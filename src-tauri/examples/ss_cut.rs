//! WZSS 索引逆向偵查：把一個 spritesheet 的全部 rect 切出來存 PNG。
//!
//! 用法：
//! ```text
//! cargo run --release --example ss_cut -- <bundle 目錄> <bundle stem> <atlas 貼圖名> <wzss .bin> <輸出目錄>
//! ```

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};
use msclassic::unity::texture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&args[0]);
    let bundle = Bundle::open(&dir.join(format!("{}.bundle", args[1])))?;
    let want_tex = &args[2];
    let wzss = std::fs::read(&args[3])?;
    let out_dir = PathBuf::from(&args[4]);
    std::fs::create_dir_all(&out_dir)?;

    // 找 atlas Texture2D 並取像素
    let mut atlas: Option<(u32, u32, Vec<u8>)> = None;
    for ni in 0..bundle.nodes.len() {
        if !bundle.nodes[ni].is_serialized_file() {
            continue;
        }
        let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
        let meta_len = metadata_len(&head48)?;
        let head = bundle.read_node_range(ni, 0, meta_len)?;
        let sf = SerializedFile::parse(&head)?;
        for o in sf.objects_by_class(objects::CLASS_TEXTURE2D) {
            let data = bundle.read_node_range(
                ni,
                sf.data_offset + o.byte_start as u64,
                o.byte_size as usize,
            )?;
            let tex = objects::parse_texture2d(&data, &sf)?;
            if tex.name == *want_tex {
                let rgba = texture::fetch_pixels(&bundle, ni, &tex, &data)?;
                atlas = Some((tex.width as u32, tex.height as u32, rgba));
                break;
            }
        }
    }
    let (aw, ah, rgba) = atlas.expect("atlas not found");
    println!("atlas {want_tex}: {aw}x{ah}");

    // WZSS：T2 = (count, rectOff) 在 header u32[7], u32[8]（offset 相對 magic）
    let magic = wzss
        .windows(4)
        .position(|w| w == b"WZSS")
        .expect("no WZSS magic");
    let hdr = |i: usize| u32::from_le_bytes(wzss[i * 4..i * 4 + 4].try_into().unwrap());
    let (cnt, off) = (hdr(7) as usize, hdr(8) as usize);
    println!("WZSS magic@{magic}，rect 區 {cnt} 筆 @ +{off}");
    for i in 0..cnt {
        let p = magic + off + i * 16;
        let x = u32::from_le_bytes(wzss[p..p + 4].try_into().unwrap());
        let y = u32::from_le_bytes(wzss[p + 4..p + 8].try_into().unwrap());
        let w = u32::from_le_bytes(wzss[p + 8..p + 12].try_into().unwrap());
        let h = u32::from_le_bytes(wzss[p + 12..p + 16].try_into().unwrap());
        let mut sub = texture::crop_rgba(aw, ah, &rgba, x, y, w, h)?;
        texture::flip_vertical(&mut sub, w, h)?;
        let png = texture::to_png(w, h, &sub)?;
        std::fs::write(out_dir.join(format!("{i:03}_{x}x{y}_{w}x{h}.png")), png)?;
    }
    println!("輸出 {cnt} 張到 {}", out_dir.display());
    Ok(())
}
