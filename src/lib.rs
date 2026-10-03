mod data_source;
mod decoder;

use std::io::Cursor;

use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};
use wasm_bindgen::prelude::*;

use self::data_source::{InputFormat, detect_format, render_cover, write_static_body};
use self::decoder::{decode_jpeg, png_to_rgba8};

pub(crate) type Error = Box<dyn std::error::Error>;

#[wasm_bindgen]
pub fn add_cover(input: &[u8]) -> Result<Vec<u8>, JsError> {
    add_cover_inner(input).map_err(|e| JsError::new(&e.to_string()))
}

fn add_cover_inner(input: &[u8]) -> Result<Vec<u8>, Error> {
    let format = detect_format(input)?;
    let mut out: Vec<u8> = Vec::new();

    match format {
        // ---------------- PNG / APNG ----------------
        InputFormat::Png => {
            let mut decoder = Decoder::new(Cursor::new(input));
            decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
            let mut reader = decoder.read_info()?;

            let (w, h) = {
                let info = reader.info();
                (info.width, info.height)
            };

            let mut enc = Encoder::new(&mut out, w, h);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);

            let (num_frames, num_plays) = match reader.info().animation_control {
                Some(a) => (a.num_frames, a.num_plays),
                None => (30, 0),
            };
            enc.set_animated(num_frames, num_plays)?;
            enc.set_sep_def_img(true)?;

            let mut writer = enc.write_header()?;
            let cover = render_cover(w as usize, h as usize)?;
            writer.write_image_data(&cover)?;

            if reader.info().animation_control.is_some() {
                // ---------- 原 APNG 分支（保持不变） ----------
                let mut buffer = vec![0; reader.output_buffer_size().unwrap()];

                if reader.info().frame_control.is_none() {
                    reader.next_frame(&mut buffer)?;
                }

                for i in 0..num_frames {
                    web_sys::console::log_1(&format!("写入帧 {i} / {num_frames}").into());

                    let frame_info = reader.next_frame(&mut buffer)?;
                    let pixels = png_to_rgba8(&frame_info, &buffer)?;

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
                // ---------- 静态 PNG ----------
                web_sys::console::log_1(&"写入单帧 (PNG)".into());

                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf)?;
                let frame = png_to_rgba8(&info, &buf)?;

                write_static_body(&mut writer, &frame, w, h, num_frames)?;
            }

            writer.finish()?;
        }

        // ---------------- JPEG ----------------
        InputFormat::Jpeg => {
            let (w, h, rgba) = decode_jpeg(input)?.ok_or("无法读取 JPEG 头信息")?;

            web_sys::console::log_1(&format!("JPEG {w}x{h} 解码完成").into());

            // JPEG 没有帧信息，按静态图处理，固定走 30 帧
            let num_frames: u32 = 30;
            let num_plays: u32 = 0;

            let mut enc = Encoder::new(&mut out, w, h);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.set_animated(num_frames, num_plays)?;
            enc.set_sep_def_img(true)?;

            let mut writer = enc.write_header()?;

            // 第 0 帧 = 封面
            let cover = render_cover(w as usize, h as usize)?;
            writer.write_image_data(&cover)?;

            // 后续帧 = 原 JPEG 内容
            write_static_body(&mut writer, &rgba, w, h, num_frames)?;

            writer.finish()?;
        }
    }

    Ok(out)
}
