//! Texture2D → RGBA8 → PNG（Phase 4 圖片管線核心）。
//!
//! 格式支援範圍 = 全量普查實測結果：整批遊戲資料只有
//! `RGB24`(fmt 3, 267 張) 與 `RGBA32`(fmt 4, 13760 張) 兩種，無 DXT/BC7/ETC。
//! 像素位置：Texture2D 本體（inline image data）或 `.resS` node（m_StreamData）。

use crate::unity::bundle::Bundle;
use crate::unity::objects::Texture2DInfo;
use crate::unity::{Result, UnityError};

/// 支援的貼圖格式（Unity TextureFormat enum 值）。
pub const FMT_ALPHA8: i32 = 1;
pub const FMT_RGB24: i32 = 3;
pub const FMT_RGBA32: i32 = 4;
pub const FMT_ARGB32: i32 = 5;
pub const FMT_BGRA32: i32 = 14;
const MAX_TEXTURE_DIMENSION: u32 = 32_768;
const MAX_DECODED_BYTES: usize = 512 * 1024 * 1024;

fn pixel_count(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 || width > MAX_TEXTURE_DIMENSION || height > MAX_TEXTURE_DIMENSION
    {
        return Err(UnityError::Format(format!(
            "貼圖尺寸異常: {width}x{height}"
        )));
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| UnityError::Format("貼圖 pixel count 溢位".into()))?;
    let rgba_bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| UnityError::Format("貼圖 RGBA 長度溢位".into()))?;
    if rgba_bytes > MAX_DECODED_BYTES {
        return Err(UnityError::Format(format!(
            "貼圖解碼後大小超過上限: {rgba_bytes} > {MAX_DECODED_BYTES}"
        )));
    }
    Ok(pixels)
}

fn required_bytes(pixels: usize, channels: usize, label: &str) -> Result<usize> {
    pixels
        .checked_mul(channels)
        .ok_or_else(|| UnityError::Format(format!("{label} 長度溢位")))
}

/// 把原始像素 bytes 轉成 RGBA8。輸入長度不符格式需求時回錯。
pub fn decode_pixels(fmt: i32, width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>> {
    let px = pixel_count(width, height)?;
    match fmt {
        FMT_RGBA32 => {
            let need = required_bytes(px, 4, "RGBA32")?;
            if data.len() < need {
                return Err(UnityError::Format(format!(
                    "RGBA32 長度不足: {} < {}",
                    data.len(),
                    need
                )));
            }
            Ok(data[..need].to_vec())
        }
        FMT_RGB24 => {
            let need = required_bytes(px, 3, "RGB24")?;
            if data.len() < need {
                return Err(UnityError::Format(format!(
                    "RGB24 長度不足: {} < {}",
                    data.len(),
                    need
                )));
            }
            let mut out = Vec::with_capacity(required_bytes(px, 4, "RGBA8")?);
            for c in data[..need].chunks_exact(3) {
                out.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
            Ok(out)
        }
        FMT_ARGB32 => {
            let need = required_bytes(px, 4, "ARGB32")?;
            if data.len() < need {
                return Err(UnityError::Format("ARGB32 長度不足".into()));
            }
            let mut out = Vec::with_capacity(need);
            for c in data[..need].chunks_exact(4) {
                out.extend_from_slice(&[c[1], c[2], c[3], c[0]]);
            }
            Ok(out)
        }
        FMT_BGRA32 => {
            let need = required_bytes(px, 4, "BGRA32")?;
            if data.len() < need {
                return Err(UnityError::Format("BGRA32 長度不足".into()));
            }
            let mut out = Vec::with_capacity(need);
            for c in data[..need].chunks_exact(4) {
                out.extend_from_slice(&[c[2], c[1], c[0], c[3]]);
            }
            Ok(out)
        }
        FMT_ALPHA8 => {
            if data.len() < px {
                return Err(UnityError::Format("Alpha8 長度不足".into()));
            }
            let mut out = Vec::with_capacity(required_bytes(px, 4, "RGBA8")?);
            for &a in &data[..px] {
                out.extend_from_slice(&[255, 255, 255, a]);
            }
            Ok(out)
        }
        other => Err(UnityError::Format(format!(
            "未支援的 TextureFormat: {other}"
        ))),
    }
}

/// 依 Texture2DInfo 從 bundle 取像素（隨機存取，只解需要的 LZ4 block）。
///
/// - `image_data_size > 0`：像素內嵌在物件 bytes 裡（`obj_data` 需為完整物件）
/// - 否則：依 m_StreamData 從 `.resS` node 取 (offset, size) 範圍
///
/// **注意：回傳的是原始（bottom-up）排列。** Unity 序列化像素是 bottom-up
/// （OpenGL 慣例），而 atlas rect 座標也是同一套 bottom-up 空間，
/// 所以正確流程是先裁切、再對最終小圖 `flip_vertical`。
pub fn fetch_pixels(
    bundle: &Bundle,
    _node_index: usize,
    tex: &Texture2DInfo,
    obj_data: &[u8],
) -> Result<Vec<u8>> {
    let raw = if tex.image_data_size > 0 {
        let img_end = tex
            .image_data_offset
            .checked_add(tex.image_data_size as usize)
            .ok_or_else(|| UnityError::Format("inline image data range 溢位".into()))?;
        obj_data
            .get(tex.image_data_offset..img_end)
            .ok_or_else(|| {
                UnityError::Format(format!(
                    "inline image data 越界: {}..{}（物件 {} bytes）",
                    tex.image_data_offset,
                    img_end,
                    obj_data.len()
                ))
            })?
            .to_vec()
    } else {
        if tex.stream_path.is_empty() {
            return Err(UnityError::Format(format!(
                "Texture2D {:?} 無像素資料（inline=0 且無 stream path）",
                tex.name
            )));
        }
        let res_name = tex.stream_path.rsplit('/').next().unwrap();
        let res_ni = bundle
            .node(res_name)
            .ok_or_else(|| UnityError::Format(format!(".resS node 不存在: {res_name}")))?;
        bundle.read_node_range(res_ni, tex.stream_offset, tex.stream_size as usize)?
    };
    let width = u32::try_from(tex.width)
        .map_err(|_| UnityError::Format(format!("Texture2D width 為負: {}", tex.width)))?;
    let height = u32::try_from(tex.height)
        .map_err(|_| UnityError::Format(format!("Texture2D height 為負: {}", tex.height)))?;
    decode_pixels(tex.texture_format, width, height, &raw)
}

/// 整張貼圖（非 atlas）取「顯示用」top-down RGBA8（fetch + 垂直翻轉）。
pub fn fetch_pixels_flipped(
    bundle: &Bundle,
    node_index: usize,
    tex: &Texture2DInfo,
    obj_data: &[u8],
) -> Result<Vec<u8>> {
    let mut rgba = fetch_pixels(bundle, node_index, tex, obj_data)?;
    flip_vertical(&mut rgba, tex.width as u32, tex.height as u32)?;
    Ok(rgba)
}

/// 垂直翻轉（bottom-up → top-down）。
pub fn flip_vertical(rgba: &mut [u8], width: u32, height: u32) -> Result<()> {
    let pixels = pixel_count(width, height)?;
    let expected = required_bytes(pixels, 4, "RGBA8")?;
    if rgba.len() != expected {
        return Err(UnityError::Format(format!(
            "flip_vertical 長度不符: {} != {expected}",
            rgba.len()
        )));
    }
    let stride = required_bytes(width as usize, 4, "scanline")?;
    for y in 0..height as usize / 2 {
        let top = y * stride;
        let bot = (height as usize - 1 - y) * stride;
        for i in 0..stride {
            rgba.swap(top + i, bot + i);
        }
    }
    Ok(())
}

/// RGBA8 → PNG（flate2 壓縮）。
pub fn to_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let pixels = pixel_count(width, height)?;
    let expected = required_bytes(pixels, 4, "PNG RGBA")?;
    if rgba.len() != expected {
        return Err(UnityError::Format(format!(
            "to_png 長度不符: {} != {}x{}x4",
            rgba.len(),
            width,
            height
        )));
    }

    // 每條掃描線前加 filter byte 0
    let row_bytes = required_bytes(width as usize, 4, "PNG scanline")?;
    let stride = row_bytes
        .checked_add(1)
        .ok_or_else(|| UnityError::Format("PNG scanline 長度溢位".into()))?;
    let raw_len = stride
        .checked_mul(height as usize)
        .ok_or_else(|| UnityError::Format("PNG raw 長度溢位".into()))?;
    let mut raw = Vec::with_capacity(raw_len);
    for row in rgba.chunks_exact(row_bytes) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(&raw).map_err(UnityError::Io)?;
    let compressed = enc.finish().map_err(UnityError::Io)?;

    let out_capacity = compressed
        .len()
        .checked_add(64)
        .ok_or_else(|| UnityError::Format("PNG 輸出長度溢位".into()))?;
    let mut out = Vec::with_capacity(out_capacity);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let write_chunk = |out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]| -> Result<()> {
        let len = u32::try_from(data.len())
            .map_err(|_| UnityError::Format("PNG chunk 超過 u32 長度".into()))?;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        let mut crc_data = Vec::with_capacity(4 + data.len());
        crc_data.extend_from_slice(tag);
        crc_data.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
        Ok(())
    };
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    write_chunk(&mut out, b"IHDR", &ihdr)?;
    write_chunk(&mut out, b"IDAT", &compressed)?;
    write_chunk(&mut out, b"IEND", &[])?;
    Ok(out)
}

/// 從 RGBA8 圖切出子矩形（atlas 切圖用）。邊界超過時回錯。
pub fn crop_rgba(
    img_w: u32,
    img_h: u32,
    rgba: &[u8],
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<Vec<u8>> {
    let img_pixels = pixel_count(img_w, img_h)?;
    let expected = required_bytes(img_pixels, 4, "crop source RGBA")?;
    if rgba.len() != expected {
        return Err(UnityError::Format(format!(
            "crop source 長度不符: {} != {expected}",
            rgba.len()
        )));
    }
    if w == 0
        || h == 0
        || x.checked_add(w).is_none_or(|end| end > img_w)
        || y.checked_add(h).is_none_or(|end| end > img_h)
    {
        return Err(UnityError::Format(format!(
            "crop 越界: ({x},{y},{w}x{h}) 超出 {img_w}x{img_h}"
        )));
    }
    let out_len = required_bytes(pixel_count(w, h)?, 4, "crop output RGBA")?;
    let row_bytes = required_bytes(w as usize, 4, "crop row")?;
    let mut out = Vec::with_capacity(out_len);
    let y_end = y + h;
    for row in y..y_end {
        let pixel_offset = (row as usize)
            .checked_mul(img_w as usize)
            .and_then(|n| n.checked_add(x as usize))
            .ok_or_else(|| UnityError::Format("crop row offset 溢位".into()))?;
        let s = required_bytes(pixel_offset, 4, "crop byte offset")?;
        let end = s
            .checked_add(row_bytes)
            .ok_or_else(|| UnityError::Format("crop row range 溢位".into()))?;
        out.extend_from_slice(
            rgba.get(s..end)
                .ok_or_else(|| UnityError::Format("crop row 越界".into()))?,
        );
    }
    Ok(out)
}

/// CRC-32（PNG 用，無外部依賴）。
fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB88320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFFFFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFFFFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_rgb24() {
        let data = [255, 0, 0, 0, 255, 0];
        let out = decode_pixels(FMT_RGB24, 2, 1, &data).unwrap();
        assert_eq!(out, vec![255, 0, 0, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn test_decode_rgba32_passthrough() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let out = decode_pixels(FMT_RGBA32, 2, 1, &data).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn test_decode_bgra32() {
        let data = [3, 2, 1, 4];
        let out = decode_pixels(FMT_BGRA32, 1, 1, &data).unwrap();
        assert_eq!(out, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_decode_argb32() {
        let data = [4, 1, 2, 3];
        let out = decode_pixels(FMT_ARGB32, 1, 1, &data).unwrap();
        assert_eq!(out, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_decode_alpha8() {
        let out = decode_pixels(FMT_ALPHA8, 2, 1, &[128, 255]).unwrap();
        assert_eq!(out, vec![255, 255, 255, 128, 255, 255, 255, 255]);
    }

    #[test]
    fn test_decode_unsupported() {
        assert!(decode_pixels(10, 1, 1, &[0; 4]).is_err()); // DXT1 不在支援範圍
    }

    #[test]
    fn test_decode_short_buffer() {
        assert!(decode_pixels(FMT_RGBA32, 2, 2, &[0; 4]).is_err());
    }

    #[test]
    fn test_decode_rejects_overflow_sized_texture() {
        assert!(decode_pixels(FMT_RGBA32, u32::MAX, u32::MAX, &[]).is_err());
        assert!(decode_pixels(FMT_RGBA32, 0, 1, &[]).is_err());
    }

    #[test]
    fn test_png_roundtrip() {
        // 2x2 紅綠藍白
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let png = to_png(2, 2, &rgba).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // IDAT zlib 可以解回來
        let idat_pos = png.windows(4).position(|w| w == b"IDAT").unwrap();
        let idat_len = u32::from_be_bytes(png[idat_pos - 4..idat_pos].try_into().unwrap()) as usize;
        let zdata = &png[idat_pos + 4..idat_pos + 4 + idat_len];
        let mut dec = flate2::read::ZlibDecoder::new(zdata);
        let mut raw = Vec::new();
        use std::io::Read;
        dec.read_to_end(&mut raw).unwrap();
        assert_eq!(raw.len(), 2 * (2 * 4 + 1));
        assert_eq!(raw[0], 0); // filter byte
        assert_eq!(&raw[1..5], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_crop() {
        let rgba: Vec<u8> = (0..4).flat_map(|i| [i, i, i, 255]).collect();
        let out = crop_rgba(4, 1, &rgba, 1, 0, 2, 1).unwrap();
        assert_eq!(out, vec![1, 1, 1, 255, 2, 2, 2, 255]);
        assert!(crop_rgba(4, 1, &rgba, 3, 0, 2, 1).is_err());
        assert!(crop_rgba(4, 1, &rgba[..4], 0, 0, 1, 1).is_err());
        assert!(crop_rgba(4, 1, &rgba, u32::MAX, 0, 2, 1).is_err());
    }

    #[test]
    fn test_flip_rejects_wrong_buffer_length() {
        assert!(flip_vertical(&mut [0; 4], 2, 2).is_err());
    }
}
