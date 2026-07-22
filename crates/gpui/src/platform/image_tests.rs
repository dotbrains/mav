use super::*;
use std::sync::Arc;

#[test]
fn test_svg_image_to_image_data_converts_to_bgra() {
    let image = Image::from_bytes(
        ImageFormat::Svg,
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">
<rect width="1" height="1" fill="#38BDF8"/>
</svg>"##
            .to_vec(),
    );

    let render_image = image.to_image_data(SvgRenderer::new(Arc::new(()))).unwrap();
    let bytes = render_image.as_bytes(0).unwrap();

    for pixel in bytes.chunks_exact(4) {
        assert_eq!(pixel, &[0xF8, 0xBD, 0x38, 0xFF]);
    }
}
