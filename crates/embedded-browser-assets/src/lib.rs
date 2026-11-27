//! Embedded fonts for `embedded-browser`.
//!
//! The font bytes are `'static`: on the device they live in flash, not on the heap.
//! Noto Sans Regular and Bold serve every generic family (sans-serif, serif, monospace,
//! system-ui); italics are synthesized. Blitz's built-in UA stylesheet is used as is.

/// Noto Sans Regular (Apache-2.0).
pub const NOTO_SANS_REGULAR: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");
/// Noto Sans Bold (Apache-2.0).
pub const NOTO_SANS_BOLD: &[u8] = include_bytes!("../fonts/NotoSans-Bold.ttf");

/// Every embedded font, registered for all generic families.
pub const FONTS: &[&[u8]] = &[NOTO_SANS_REGULAR, NOTO_SANS_BOLD];

#[cfg(test)]
mod tests {
    #[test]
    fn fonts_are_truetype() {
        for f in super::FONTS {
            assert_eq!(&f[..4], &[0, 1, 0, 0]);
        }
    }
}
