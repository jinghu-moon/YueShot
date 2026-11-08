//! The overlay's typeface: the embedded subset, the family stack that goes with it, and the
//! explicit DirectWrite collection and private GDI registration.
//!
//! DirectWrite reads the embedded bytes through [`collection_for`], independently of the
//! machine's installed fonts. GPUI windows use their own system-font configuration.
//!
//! The subset is embedded verbatim and registered from memory: `AddFontMemResourceEx` loads it
//! straight from the bytes baked into the binary — no temp file, no system install, no admin
//! rights. The slice is `'static`, so the registration stays valid for the whole process.

use ::windows::Win32::Graphics::Gdi::AddFontMemResourceEx;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_ISOLATED, DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection,
    IDWriteInMemoryFontFileLoader,
};
use windows::core::{Interface, Result};

/// The overlay's typeface: a subsetted Noto Sans SC carrying exactly the characters the
/// overlay can draw — every label, hint, guide row and key name, under any binding
/// (`overlay_drawn_strings` in the tests, built by `subfont/subset.ps1`).
pub const SUBSET: &[u8] = include_bytes!("../assets/noto-sans-sc-subset.ttf");

/// Family name the embedded subset registers under (its family entry is set explicitly during
/// subsetting while copyright and license metadata are retained). The name itself is owned by the ui crate, so the save panel
/// and the overlay cannot drift apart; only the registration and the bytes live here.
pub const FAMILY: &str = crate::panel::FAMILY;

/// The stock UI families to fall through to, in order, when [`FAMILY`] cannot be resolved.
pub const FALLBACKS: [&str; 2] = ["Segoe UI Variable Display", "Segoe UI"];

/// Every family a text run may end up in, in priority order.
pub const FAMILIES: [&str; 3] = [FAMILY, FALLBACKS[0], FALLBACKS[1]];

/// Shared metrics for the zoom badge drawn by DirectWrite and the pin window's GDI fallback.
///
/// Keeping these values beside the shared family stack prevents the two renderers from silently
/// drifting apart again. `700` is DirectWrite's BOLD weight and GDI's `FW_BOLD` weight.
pub const ZOOM_BADGE_FONT_SIZE_DIP: f32 = 14.0;
pub const ZOOM_BADGE_FONT_WEIGHT: i32 = 700;

/// Register the embedded subsets privately for GDI controls in this process, once.
///
/// A failure is not fatal and has no one to report to: every text run falls back through
/// [`FAMILIES`] when the embedded family cannot be resolved.
///
/// GDI memory registration does not add fonts to DirectWrite's system collection.
/// DirectWrite callers must use [`collection_for`] instead.
pub fn register() {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| unsafe {
        let mut installed = 0u32;
        let _ = AddFontMemResourceEx(
            SUBSET.as_ptr().cast(),
            SUBSET.len() as u32,
            None,
            &raw mut installed,
        );
    });
    crate::panel::register_number_font();
}

// The loader must remain registered for as long as any format/face uses the font files.
// Both fonts and their owning factory/loader have process lifetime, like their static bytes.
struct EmbeddedFonts {
    collection: IDWriteFontCollection,
    _factory: IDWriteFactory5,
    _loader: IDWriteInMemoryFontFileLoader,
}

impl EmbeddedFonts {
    fn load() -> Result<Self> {
        unsafe {
            let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED)?;
            let loader = factory.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            let result = (|| {
                let builder = factory.CreateFontSetBuilder()?;
                for bytes in [SUBSET, crate::panel::NUMBER_SUBSET] {
                    let file = loader.CreateInMemoryFontFileReference(
                        &factory,
                        bytes.as_ptr().cast(),
                        bytes.len() as u32,
                        None,
                    )?;
                    builder.AddFontFile(&file)?;
                }
                let fonts = builder.CreateFontSet()?;
                let collection = factory.CreateFontCollectionFromFontSet(&fonts)?.cast()?;
                Ok(collection)
            })();
            match result {
                Ok(collection) => Ok(Self {
                    collection,
                    _factory: factory,
                    _loader: loader,
                }),
                Err(error) => {
                    let _ = factory.UnregisterFontFileLoader(&loader);
                    Err(error)
                }
            }
        }
    }
}

/// Return the explicit embedded collection for one of our two embedded families.
/// Stock families continue to use the caller's system collection. Loading errors propagate;
/// an installed copy of the same font must never hide invalid embedded bytes.
pub fn collection_for(family: &str) -> Result<Option<IDWriteFontCollection>> {
    if family != FAMILY && family != crate::panel::NUMBER_FAMILY {
        return Ok(None);
    }
    static FONTS: std::sync::OnceLock<Result<EmbeddedFonts>> = std::sync::OnceLock::new();
    match FONTS.get_or_init(EmbeddedFonts::load) {
        Ok(fonts) => Ok(Some(fonts.collection.clone())),
        Err(error) => Err(error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_FACTORY_TYPE_ISOLATED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_WEIGHT_NORMAL, DWriteCreateFactory, IDWriteFactory,
    };
    use windows::core::{BOOL, PCWSTR};

    #[test]
    fn embedded_fonts_resolve_in_directwrite_with_regular_weight_and_required_glyphs() {
        let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED) }
            .expect("DirectWrite factory");
        let collection = collection_for(FAMILY).unwrap().unwrap();
        assert_eq!(unsafe { collection.GetFontFamilyCount() }, 2);
        assert!(collection_for("Segoe UI").unwrap().is_none());
        let mut ui_text = crate::panel::form_drawn_strings().join("");
        ui_text.extend(' '..='~');
        for (family, text) in [
            (FAMILY, ui_text.as_str()),
            (
                crate::panel::NUMBER_FAMILY,
                crate::panel::NUMBER_CHARACTERS.trim(),
            ),
        ] {
            let wide: Vec<u16> = family.encode_utf16().chain(Some(0)).collect();
            let mut index = 0;
            let mut exists = BOOL(0);
            unsafe { collection.FindFamilyName(PCWSTR(wide.as_ptr()), &mut index, &mut exists) }
                .unwrap();
            assert!(exists.as_bool(), "embedded family not found: {family}");
            let font = unsafe {
                collection
                    .GetFontFamily(index)
                    .unwrap()
                    .GetFirstMatchingFont(
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        DWRITE_FONT_STYLE_NORMAL,
                    )
            }
            .unwrap();
            assert_eq!(unsafe { font.GetWeight() }, DWRITE_FONT_WEIGHT_NORMAL);
            let face = unsafe { font.CreateFontFace() }.unwrap();
            let codes: Vec<u32> = text.chars().map(u32::from).collect();
            let mut glyphs = vec![0u16; codes.len()];
            unsafe {
                face.GetGlyphIndices(codes.as_ptr(), codes.len() as u32, glyphs.as_mut_ptr())
            }
            .unwrap();
            assert!(
                glyphs.iter().all(|glyph| *glyph != 0),
                "missing glyph: {family}"
            );
            let format = crate::panel::text_format_in(
                &factory,
                96.0,
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                &[family],
            )
            .unwrap();
            let used = unsafe { format.GetFontCollection() }.unwrap();
            assert_eq!(unsafe { used.GetFontFamilyCount() }, 2);
        }
    }
}
