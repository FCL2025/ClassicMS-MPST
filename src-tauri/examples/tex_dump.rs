//! 抽取單張 Texture2D 的原始像素到檔案（hash 假設驗證用）。
//! 用法：cargo run --release --example tex_dump -- <bundle 目錄> <bundle stem> <貼圖名> <輸出.raw>

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&args[0]);
    let path = dir.join(format!("{}.bundle", args[1]));
    let want = &args[2];
    let out = &args[3];

    let bundle = Bundle::open(&path)?;
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
            if tex.name != *want {
                continue;
            }
            println!(
                "found {:?}: {}x{} fmt={} inline={} stream=({},{} {:?})",
                tex.name,
                tex.width,
                tex.height,
                tex.texture_format,
                tex.image_data_size,
                tex.stream_offset,
                tex.stream_size,
                tex.stream_path
            );
            let pixels = if tex.image_data_size > 0 {
                // 內嵌像素：image data 在物件 bytes 裡（m_CompleteImageSize 長度）
                let img_off = data.len() - tex.image_data_size as usize;
                data[img_off..].to_vec()
            } else {
                // .resS node：stream_path 最後一段是 node 名
                let res_name = tex.stream_path.rsplit('/').next().unwrap();
                let res_ni = bundle.node(res_name).expect("resS node 不存在");
                bundle.read_node_range(res_ni, tex.stream_offset, tex.stream_size as usize)?
            };
            println!("pixels {} bytes -> {out}", pixels.len());
            std::fs::write(out, pixels)?;
            return Ok(());
        }
    }
    eprintln!("texture {want} not found");
    std::process::exit(1);
}
