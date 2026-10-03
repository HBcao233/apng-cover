//! 把原来 CLI 版 main() 改造成 WASM 入口。
//!
//! 输入：PNG（静态或 APNG）字节
//! 输出：重新生成的 APNG 字节（第 0 帧是内置封面图，后面跟随原动画帧）

use std::io::Cursor;

use png::{
    BitDepth, BlendOp, ColorType, Decoder, DisposeOp, Encoder, OutputInfo, Transformations,
};
use wasm_bindgen::prelude::*;

/// 编译期嵌入的封面图（放在 src/cover.png）
const COVER_DATA: &[u8] = include_bytes!("cover.png");

/// 画布背景色
const BG: [u8; 4] = [0xf1, 0xf1, 0xf2, 0xff];

type Error = Box<dyn std::error::Error>;

// ---------------------------------------------------------------------------
// 把解码后的 RGB、灰度等数据统一转换为 RGBA8
// ---------------------------------------------------------------------------
fn to_rgba8(info: &OutputInfo, buffer: &[u8]) -> Result<Vec<u8>, Error> {
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
fn render_cover(width: usize, height: usize) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(Cursor::new(COVER_DATA));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;

    let mut buf = vec![0u8; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf)?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = to_rgba8(&info, &buf)?;

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

// ---------------------------------------------------------------------------
// WASM 导出入口
// ---------------------------------------------------------------------------

/// 输入 PNG 字节，返回重新生成的 APNG 字节。
/// 出错时抛出 JS 异常。
#[wasm_bindgen]
pub fn generate_cover(input: &[u8]) -> Result<Vec<u8>, JsError> {
    generate_cover_inner(input).map_err(|e| JsError::new(&e.to_string()))
}

/// 真正的实现：把原 main() 里的逻辑从「读文件 / 写文件」换成「读内存 / 写内存」。
fn generate_cover_inner(input: &[u8]) -> Result<Vec<u8>, Error> {
    // ---- 相当于 open_png(path) ----
    let mut decoder = Decoder::new(Cursor::new(input));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;

    let (w, h) = {
        let info = reader.info();
        (info.width, info.height)
    };

    // ---- 相当于 File::create(output_path) + Encoder::new(...) ----
    let mut out: Vec<u8> = Vec::new();

    {
        let mut enc = Encoder::new(&mut out, w, h);
        enc.set_color(ColorType::Rgba);
        enc.set_depth(BitDepth::Eight);

        let (num_frames, num_plays) = match reader.info().animation_control {
            Some(a) => (a.num_frames, a.num_plays),
            None => (30, 1),
        };
        enc.set_animated(num_frames, num_plays)?;
        enc.set_sep_def_img(true)?;

        let mut writer = enc.write_header()?;
        let cover = render_cover(w as usize, h as usize)?;

        // 第 0 帧 = 封面
        writer.write_image_data(&cover)?;

        if reader.info().animation_control.is_some() {
            // ---------- APNG 分支 ----------
            let mut buffer = vec![0; reader.output_buffer_size().unwrap()];

            // 原 APNG 如果有独立默认图，跳过它，由新封面替换。
            // 没有 fcTL 的第一张 IDAT 图片不属于动画帧。
            if reader.info().frame_control.is_none() {
                reader.next_frame(&mut buffer)?;
            }

            for i in 0..num_frames {
                web_sys::console::log_1(&format!("写入帧 {i} / {num_frames}").into());

                let frame_info = reader.next_frame(&mut buffer)?;
                let pixels = to_rgba8(&frame_info, &buffer)?;

                let control = reader
                    .info()
                    .frame_control
                    .ok_or("动画帧缺少 fcTL 控制信息")?;

                writer.set_frame_dimension(control.width, control.height)?;
                writer.set_frame_position(control.x_offset, control.y_offset)?;
                writer.set_blend_op(control.blend_op)?;
                writer.set_dispose_op(control.dispose_op)?;

                writer.set_frame_delay(control.delay_num, control.delay_den)?;
                writer.write_image_data(&pixels)?;
            }
        } else {
            // ---------- 静态图分支 ----------
            web_sys::console::log_1(&"写入单帧".into());

            let mut buf = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut buf)?;
            let frame = to_rgba8(&info, &buf)?;

            writer.set_frame_dimension(w, h)?;
            writer.set_frame_position(0, 0)?;
            writer.set_blend_op(BlendOp::Source)?;
            writer.set_dispose_op(DisposeOp::None)?;

            writer.set_frame_delay(1, 30)?;
            writer.write_image_data(&frame)?;

            writer.set_frame_delay(1, 30)?;
            writer.write_image_data(&frame)?;

            let transparent = vec![255, 255, 255, 0].repeat(50 * 50);
            for _ in 2..num_frames {
                writer.set_frame_dimension(50, 50)?;
                writer.set_frame_position(0, 0)?;
                writer.set_frame_delay(1, 30)?;
                writer.set_blend_op(BlendOp::Over)?;
                writer.set_dispose_op(DisposeOp::None)?;
                writer.write_image_data(&transparent)?;
            }
        }

        writer.finish()?;
    }

    Ok(out)
}