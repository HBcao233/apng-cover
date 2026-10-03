use png::{BitDepth, ColorType, OutputInfo};

// ---------------------------------------------------------------------------
// 把解码后的 RGB、灰度等数据统一转换为 RGBA8
// ---------------------------------------------------------------------------
pub(crate) fn png_to_rgba8(info: &OutputInfo, buffer: &[u8]) -> Result<Vec<u8>, crate::Error> {
    if info.bit_depth != BitDepth::Eight {
        return Err("解码后的图片不是 8 位数据".into());
    }

    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buffer[..info.buffer_size()];
    let mut rgba = Vec::with_capacity(w * h * 4);

    match info.color_type {
        ColorType::Rgba => return Ok(data.to_vec()),

        ColorType::Rgb => {
            for p in data.chunks_exact(3) {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }

        ColorType::Grayscale => {
            for &g in data {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }

        ColorType::GrayscaleAlpha => {
            for p in data.chunks_exact(2) {
                rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }

        ColorType::Indexed => {
            return Err("调色板数据未被展开".into());
        }
    }

    Ok(rgba)
}
