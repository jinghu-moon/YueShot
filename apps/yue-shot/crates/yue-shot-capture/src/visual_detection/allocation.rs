//! Named allocation guards for the selected path. Injection is test-only;
//! production Vec growth is fallible and bounded by its caller's size contract.
#[cfg(test)]
std::thread_local! {
    static FAIL: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
    static HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn permit(_site: &'static str) -> bool {
    #[cfg(test)]
    if FAIL.get() == Some(_site) {
        HIT.set(true);
        return false;
    }
    true
}

pub(crate) fn reserve<T>(values: &mut Vec<T>, additional: usize, site: &'static str) -> bool {
    if additional <= values.capacity().saturating_sub(values.len()) {
        return true;
    }
    permit(site) && values.try_reserve(additional).is_ok()
}

#[cfg(test)]
pub(crate) fn fail_at<T>(site: &'static str, run: impl FnOnce() -> T) -> (T, bool) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAIL.set(None);
        }
    }
    FAIL.set(Some(site));
    HIT.set(false);
    let _reset = Reset;
    let output = run();
    (output, HIT.get())
}
