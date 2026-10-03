use std::io::Cursor;

use png::{BlendOp, Decoder, DisposeOp, Transformations};

use crate::decoder::png_to_rgba8;

/// 编译期嵌入的封面图（放在 src/cover.png）
const COVER_DATA: &[u8] = include_bytes!("cover.png");

/// 画布背景色
const BG: [u8; 4] = [0xf1, 0xf1, 0xf2, 0xff];

/// 面积平均缩放（适合缩小；使用预乘 alpha，避免透明边缘发黑）
fn resize_area(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 4];
    let x_scale = sw as f64 / dw as f64;
    let y_scale = sh as f64 / dh as f64;

    for dy in 0..dh {
        let y0 = dy as f64 * y_scale;
        let y1 = y0 + y_scale;
        for dx in 0..dw {
            let x0 = dx as f64 * x_scale;
            let x1 = x0 + x_scale;

            let (mut r, mut g, mut b, mut a, mut wsum) = (0f64, 0f64, 0f64, 0f64, 0f64);

            for sy in (y0.floor() as usize)..(y1.ceil() as usize).min(sh) {
                let wy = (y1.min(sy as f64 + 1.0) - y0.max(sy as f64)).max(0.0);
                for sx in (x0.floor() as usize)..(x1.ceil() as usize).min(sw) {
                    let wx = (x1.min(sx as f64 + 1.0) - x0.max(sx as f64)).max(0.0);
                    let w = wx * wy;
                    let i = (sy * sw + sx) * 4;
                    let pa = src[i + 3] as f64 / 255.0;
                    r += src[i] as f64 * pa * w;
                    g += src[i + 1] as f64 * pa * w;
                    b += src[i + 2] as f64 * pa * w;
                    a += pa * w;
                    wsum += w;
                }
            }

            let o = (dy * dw + dx) * 4;
            if a > 0.0 && wsum > 0.0 {
                out[o] = (r / a).round().clamp(0.0, 255.0) as u8;
                out[o + 1] = (g / a).round().clamp(0.0, 255.0) as u8;
                out[o + 2] = (b / a).round().clamp(0.0, 255.0) as u8;
                out[o + 3] = (a / wsum * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// 把内置封面图缩放到 width×height 的画布上（居中，其余填背景色）
pub(crate) fn render_cover(width: usize, height: usize) -> Result<Vec<u8>, crate::Error> {
    let mut decoder = Decoder::new(Cursor::new(COVER_DATA));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;

    let mut buf = vec![0u8; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = png_to_rgba8(&info, &buf)?;

    let side = width.min(height);
    let scaled = resize_area(&rgba, w, h, side, side);

    let mut canvas = BG.repeat(width * height);

    let off_x = (width - side) / 2;
    let off_y = (height - side) / 2;

    for y in 0..side {
        let src_start = y * side * 4;
        let src_end = src_start + side * 4;

        let dst_y = off_y + y;
        let dst_start = (dst_y * width + off_x) * 4;
        let dst_end = dst_start + side * 4;

        canvas[dst_start..dst_end].copy_from_slice(&scaled[src_start..src_end]);
    }

    Ok(canvas)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputFormat {
    Png,
    Jpeg,
}

pub(crate) fn detect_format(data: &[u8]) -> Result<InputFormat, crate::Error> {
    const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if data.len() >= 8 && data[..8] == PNG_SIG {
        Ok(InputFormat::Png)
    } else if data.len() >= 3 && data[..3] == [0xFF, 0xD8, 0xFF] {
        Ok(InputFormat::Jpeg)
    } else {
        Err("不支持的图片格式：仅支持 PNG / APNG / JPEG".into())
    }
}

/// 封面帧之后写入的静态图动画尾巴：
/// 原图重复两帧 + 透明小帧补足到 num_frames。
pub(crate) fn write_static_body<W: std::io::Write>(
    writer: &mut png::Writer<W>,
    frame: &[u8],
    w: u32,
    h: u32,
    num_frames: u32,
) -> Result<(), crate::Error> {
    writer.set_frame_dimension(w, h)?;
    writer.set_frame_position(0, 0)?;
    writer.set_blend_op(BlendOp::Source)?;
    writer.set_dispose_op(DisposeOp::None)?;

    writer.set_frame_delay(1, 30)?;
    writer.write_image_data(frame)?;
    writer.set_frame_delay(1, 30)?;
    writer.write_image_data(frame)?;

    let transparent = vec![255u8, 255, 255, 0].repeat(50 * 50);
    for _ in 2..num_frames {
        writer.set_frame_dimension(50, 50)?;
        writer.set_frame_position(0, 0)?;
        writer.set_frame_delay(1, 30)?;
        writer.set_blend_op(BlendOp::Over)?;
        writer.set_dispose_op(DisposeOp::None)?;
        writer.write_image_data(&transparent)?;
    }
    Ok(())
}
