use std::io::Cursor;

/// 解码 JPEG，返回 (width, height, rgba8)
pub(crate) fn decode_jpeg(
    input: &[u8],
) -> Result<Option<(u32, u32, Vec<u8>)>, jpeg_decoder::Error> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(input));
    let pixels = decoder.decode()?;
    Ok(if let Some(info) = decoder.info() {
        let rgba = jpeg_to_rgba8(pixels, info.pixel_format);
        Some((info.width as u32, info.height as u32, rgba))
    } else {
        None
    })
}

fn jpeg_to_rgba8(pixels: Vec<u8>, fmt: jpeg_decoder::PixelFormat) -> Vec<u8> {
    use jpeg_decoder::PixelFormat::*;
    match fmt {
        RGB24 => {
            let mut out = Vec::with_capacity(pixels.len() / 3 * 4);
            for p in pixels.chunks_exact(3) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            out
        }
        L8 => {
            let mut out = Vec::with_capacity(pixels.len() * 4);
            for &g in &pixels {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        L16 => {
            // 16 位灰度，大端序，取高 8 位即可
            let mut out = Vec::with_capacity(pixels.len() / 2 * 4);
            for p in pixels.chunks_exact(2) {
                let g = p[0];
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        CMYK32 => {
            let mut out = Vec::with_capacity(pixels.len() * 4);
            for p in pixels.chunks_exact(4) {
                let c = p[0] as f32 / 255.0;
                let m = p[1] as f32 / 255.0;
                let y = p[2] as f32 / 255.0;
                let k = p[3] as f32 / 255.0;
                out.push((255.0 * (1.0 - c) * (1.0 - k)).round() as u8);
                out.push((255.0 * (1.0 - m) * (1.0 - k)).round() as u8);
                out.push((255.0 * (1.0 - y) * (1.0 - k)).round() as u8);
                out.push(255);
            }
            out
        }
    }
}
