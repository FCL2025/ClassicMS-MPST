//! Phase 4 偵查：貼圖格式普查 + spritesheet 索引 MonoBehaviour 結構分析。
//!
//! 用法：
//! ```text
//! cargo run --release --example tex_probe -- <bundle 目錄>
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap());
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            (n.starts_with("spriteset_")
                || n.starts_with("spritesheet_")
                || n.starts_with("atlas_"))
                && p.extension().is_some_and(|x| x == "bundle")
        })
        .collect();
    files.sort();

    let mut fmt_stats: BTreeMap<i32, usize> = BTreeMap::new();
    let mut stream_stats: BTreeMap<String, usize> = BTreeMap::new();

    for path in &files {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let bundle = Bundle::open(path)?;
        let mut tex_count = 0;
        let mut res_nodes: Vec<String> = vec![];
        for (ni, node) in bundle.nodes.iter().enumerate() {
            if !node.is_serialized_file() {
                res_nodes.push(node.name.clone());
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
                *fmt_stats.entry(tex.texture_format).or_default() += 1;
                let key = if tex.image_data_size > 0 {
                    "inline".to_string()
                } else {
                    format!(
                        "stream path={:?}",
                        tex.stream_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&tex.stream_path)
                    )
                };
                *stream_stats.entry(key).or_default() += 1;
                if tex_count < 2 {
                    println!(
                        "[{stem}] {:?}: {}x{} fmt={} mip={} imgData={} stream=({},{} {:?})",
                        tex.name,
                        tex.width,
                        tex.height,
                        tex.texture_format,
                        tex.mip_count,
                        tex.image_data_size,
                        tex.stream_offset,
                        tex.stream_size,
                        tex.stream_path
                    );
                }
                tex_count += 1;
            }

            // spritesheet 索引 MonoBehaviour：dump 前 2 筆前 192 bytes
            if stem.starts_with("spritesheet_") {
                for o in sf.objects_by_class(objects::CLASS_MONO_BEHAVIOUR).take(1) {
                    let data = bundle.read_node_range(
                        ni,
                        sf.data_offset + o.byte_start as u64,
                        o.byte_size as usize,
                    )?;
                    let mb = objects::parse_mono_behaviour(&data, &sf)?;
                    println!(
                        "[{stem}] 索引 MonoBehaviour {:?} payload {} bytes:",
                        mb.name,
                        data.len() - mb.header_len
                    );
                    let payload = &data[mb.header_len..];
                    for (row, chunk) in payload.chunks(16).take(12).enumerate() {
                        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                        let ascii: String = chunk
                            .iter()
                            .map(|&b| {
                                if (0x20..0x7f).contains(&b) {
                                    b as char
                                } else {
                                    '.'
                                }
                            })
                            .collect();
                        println!("  {:06x}: {:<47}  {}", row * 16, hex.join(" "), ascii);
                    }
                    // 試 WZJS magic
                    if payload.len() > 148 {
                        println!("  offset144 magic: {:?}", &payload[144..148]);
                    }
                }
            }
        }
        if tex_count > 0 {
            println!("[{stem}] Texture2D 共 {tex_count} 個；非 serialized nodes: {res_nodes:?}");
        }
    }

    println!("\n=== TextureFormat 分佈 ===");
    for (f, c) in &fmt_stats {
        println!("  fmt {f}: {c} 張");
    }
    println!("\n=== 像素位置分佈 ===");
    for (k, c) in &stream_stats {
        println!("  {k}: {c} 張");
    }
    Ok(())
}
