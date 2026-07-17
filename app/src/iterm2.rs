use anyhow::Result;
use base64::Engine;
use std::io::Cursor;

/// Whether the given `TERM_PROGRAM` value identifies iTerm2, which understands the
/// OSC 1337 inline images protocol.
pub fn is_supported(term_program: Option<&str>) -> bool {
    term_program == Some("iTerm.app")
}

/// Reads the real `TERM_PROGRAM` environment variable to decide iTerm2 support.
pub fn detect_support() -> bool {
    is_supported(std::env::var("TERM_PROGRAM").ok().as_deref())
}

/// Builds an OSC 1337 inline-image escape sequence that asks the terminal to display
/// `png_bytes` scaled to fit exactly `width_cells` x `height_cells` character cells,
/// preserving the image's aspect ratio within that box.
pub fn image_escape_sequence(png_bytes: &[u8], width_cells: u16, height_cells: u16) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    format!(
        "\x1b]1337;File=inline=1;width={width_cells};height={height_cells};preserveAspectRatio=1:{encoded}\x07"
    )
}

/// Encodes an in-memory image as PNG bytes, suitable for embedding in an
/// [`image_escape_sequence`].
pub fn encode_png(image: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_supported_true_for_iterm2() {
        assert!(is_supported(Some("iTerm.app")));
    }

    #[test]
    fn is_supported_false_for_other_terminal_programs() {
        assert!(!is_supported(Some("Apple_Terminal")));
        assert!(!is_supported(Some("WezTerm")));
    }

    #[test]
    fn is_supported_false_when_unset() {
        assert!(!is_supported(None));
    }

    #[test]
    fn image_escape_sequence_starts_with_osc_1337_file_header() {
        let escape = image_escape_sequence(b"fake-png-bytes", 10, 5);
        assert!(
            escape.starts_with("\x1b]1337;File=inline=1;"),
            "expected OSC 1337 header, got: {escape:?}"
        );
    }

    #[test]
    fn image_escape_sequence_includes_width_and_height_in_cells() {
        let escape = image_escape_sequence(b"fake-png-bytes", 42, 17);
        assert!(
            escape.contains("width=42;height=17;"),
            "expected width/height in cells, got: {escape:?}"
        );
    }

    #[test]
    fn image_escape_sequence_ends_with_bel_terminator() {
        let escape = image_escape_sequence(b"fake-png-bytes", 10, 5);
        assert!(escape.ends_with('\x07'), "expected BEL terminator, got: {escape:?}");
    }

    #[test]
    fn image_escape_sequence_base64_encodes_the_given_bytes() {
        let original = b"some arbitrary bytes \x00\x01\xff not valid utf8";
        let escape = image_escape_sequence(original, 10, 5);

        let payload = escape
            .strip_prefix("\x1b]1337;File=inline=1;width=10;height=5;preserveAspectRatio=1:")
            .and_then(|rest| rest.strip_suffix('\x07'))
            .expect("escape sequence should have the expected header/terminator shape");

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("payload should be valid base64");
        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_png_roundtrips_pixel_data() {
        use image::{DynamicImage, ImageBuffer, Rgb};
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(3, 2, |_, _| Rgb([10, 20, 30])));

        let bytes = encode_png(&img).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();

        assert_eq!(decoded.width(), 3);
        assert_eq!(decoded.height(), 2);
        let pixel = decoded.to_rgb8().get_pixel(0, 0).0;
        assert_eq!(pixel, [10, 20, 30]);
    }
}
