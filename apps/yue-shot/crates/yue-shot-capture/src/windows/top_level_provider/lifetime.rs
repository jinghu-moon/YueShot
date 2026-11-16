//! Session-private window properties distinguish same-PID/class HWND reuse.
//! Values are scalar tags, never pointers or kernel handles; do not CloseHandle them.
use crate::window_detection::WindowIdentity;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::{
    Foundation::{HANDLE, HWND},
    UI::WindowsAndMessaging::{GetPropW, RemovePropW, SetPropW},
};
use windows::core::{GUID, PCWSTR};

static NEXT_TAG: AtomicUsize = AtomicUsize::new(1);
// 64 KiB of records on 64-bit Windows. Excess owners remain native-only.
const MAX_TAGS: usize = 4096;

#[derive(Debug)]
pub(super) struct LifetimeTags {
    name: Option<[u16; 48]>,
    records: Vec<(isize, usize)>,
}

impl Default for LifetimeTags {
    fn default() -> Self {
        let name = GUID::new().ok().map(|id| {
            // Fixed storage: prefix + 32 hex digits + NUL, without a heap allocation.
            let mut name = [0; 48];
            let prefix = b"YueShot.Owner.";
            for (dst, src) in name.iter_mut().zip(prefix) {
                *dst = u16::from(*src);
            }
            for i in 0..32 {
                let digit = ((id.to_u128() >> (4 * (31 - i))) & 15) as usize;
                name[prefix.len() + i] = u16::from(b"0123456789abcdef"[digit]);
            }
            name
        });
        Self {
            name,
            records: Vec::new(),
        }
    }
}

impl LifetimeTags {
    fn property(&self, hwnd: isize) -> usize {
        self.name.as_ref().map_or(0, |name| unsafe {
            GetPropW(HWND(hwnd as _), PCWSTR(name.as_ptr())).0 as usize
        })
    }

    pub(super) fn matches(&self, identity: WindowIdentity) -> bool {
        identity.creation_token != 0 && self.property(identity.hwnd) == identity.creation_token
    }

    pub(super) fn tag(&mut self, hwnd: isize) -> Option<usize> {
        let name = self.name.as_ref()?;
        if let Some(&(_, tag)) = self.records.iter().find(|&&(h, _)| h == hwnd) {
            if self.property(hwnd) == tag {
                return Some(tag);
            }
            // The original window was destroyed, or its tag was removed. Never reuse it.
            self.records.retain(|&(h, _)| h != hwnd);
        }
        if self.records.len() == MAX_TAGS {
            return None;
        }
        if !crate::visual_detection::allocation::permit("owner_lifetime_tags")
            || self.records.try_reserve_exact(1).is_err()
        {
            return None;
        }
        let tag = NEXT_TAG
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                n.checked_add(1).filter(|&next| next <= isize::MAX as usize)
            })
            .ok()?;
        unsafe {
            SetPropW(
                HWND(hwnd as _),
                PCWSTR(name.as_ptr()),
                Some(HANDLE(tag as _)),
            )
        }
        .ok()?;
        self.records.push((hwnd, tag));
        (self.property(hwnd) == tag).then_some(tag)
    }

    /// Bound lifetime to this snapshot/session, removing only properties still ours.
    pub(super) fn retain(&mut self, keep: impl Fn(isize) -> bool) {
        let Some(name) = self.name.as_ref() else {
            return;
        };
        self.records.retain(|&(hwnd, tag)| {
            let current = unsafe { GetPropW(HWND(hwnd as _), PCWSTR(name.as_ptr())) }.0 as usize;
            if current != tag {
                return false;
            }
            if keep(hwnd) {
                return true;
            }
            let _ = unsafe { RemovePropW(HWND(hwnd as _), PCWSTR(name.as_ptr())) };
            false
        });
    }
}

impl Drop for LifetimeTags {
    fn drop(&mut self) {
        self.retain(|_| false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
        },
        core::w,
    };

    // Hidden STATIC windows: API/lifecycle contract, not visible desktop acceptance.
    struct Hidden(HWND);
    impl Hidden {
        fn new() -> Self {
            Self(
                unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        w!("STATIC"),
                        w!("YueShot lifecycle test"),
                        WINDOW_STYLE(0),
                        0,
                        0,
                        32,
                        32,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                .expect("create hidden API fixture"),
            )
        }
        fn handle(&self) -> isize {
            self.0.0 as isize
        }
    }
    impl Drop for Hidden {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) }.unwrap();
        }
    }

    #[test]
    fn owner_lifetime_stable_until_property_loss_then_new_tag() {
        let window = Hidden::new();
        let mut tags = LifetimeTags::default();
        let first = tags.tag(window.handle()).unwrap();
        assert_eq!(tags.tag(window.handle()), Some(first));
        let old = WindowIdentity::new(window.handle(), std::process::id(), 1, first);
        assert!(tags.matches(old));
        unsafe { RemovePropW(window.0, PCWSTR(tags.name.as_ref().unwrap().as_ptr())) }.unwrap();
        assert!(!tags.matches(old));
        let second = tags.tag(window.handle()).unwrap();
        assert_ne!(first, second);
        assert!(!tags.matches(old));
        assert_eq!(tags.records.len(), 1);
    }

    #[test]
    fn owner_lifetime_destroy_does_not_transfer_property_to_next_window() {
        let mut tags = LifetimeTags::default();
        let window = Hidden::new();
        let hwnd = window.handle();
        let tag = tags.tag(hwnd).unwrap();
        drop(window);
        assert_eq!(tags.property(hwnd), 0);
        let next = Hidden::new();
        assert_eq!(tags.property(next.handle()), 0);
        assert_ne!(tags.tag(next.handle()), Some(tag));
        // Windows controls actual HWND recycling; do not pretend the two were equal.
        tags.retain(|h| h == next.handle());
        assert_eq!(tags.records.len(), 1);
    }

    #[test]
    fn owner_lifetime_providers_reset_and_drop_are_isolated() {
        let window = Hidden::new();
        let mut a = LifetimeTags::default();
        let mut b = LifetimeTags::default();
        let first = a.tag(window.handle()).unwrap();
        let second = b.tag(window.handle()).unwrap();
        assert_ne!(a.name, b.name);
        assert_ne!(first, second);
        let a_name = a.name.unwrap();
        a.retain(|_| false);
        assert_eq!(a.property(window.handle()), 0);
        assert_eq!(b.property(window.handle()), second);
        let third = a.tag(window.handle()).unwrap();
        assert_ne!(first, third);
        drop(a);
        assert_eq!(
            unsafe { GetPropW(window.0, PCWSTR(a_name.as_ptr())) }.0 as usize,
            0
        );
        assert_eq!(b.property(window.handle()), second);
    }

    #[test]
    fn owner_lifetime_capability_and_allocation_failure_leave_no_tag() {
        let window = Hidden::new();
        let mut tags = LifetimeTags::default();
        let (result, hit) =
            crate::visual_detection::allocation::fail_at("owner_lifetime_tags", || {
                tags.tag(window.handle())
            });
        assert!(hit && result.is_none());
        assert_eq!(tags.property(window.handle()), 0);
        assert!(tags.records.is_empty());
        tags.name = None;
        assert_eq!(tags.tag(window.handle()), None);
        assert_eq!(tags.tag(0), None);
    }

    #[test]
    fn owner_lifetime_capacity_and_invalid_handle_fail_closed() {
        let window = Hidden::new();
        let mut tags = LifetimeTags::default();
        assert_eq!(tags.tag(0), None);
        assert!(tags.records.is_empty());
        tags.records.resize(MAX_TAGS, (0, 1));
        assert_eq!(tags.tag(window.handle()), None);
        assert_eq!(tags.property(window.handle()), 0);
        tags.records.clear();
        let token = tags.tag(window.handle()).unwrap();
        assert!(!tags.matches(WindowIdentity::new(window.handle(), 0, 0, 0)));
        assert_ne!(token, 0);
    }
}
