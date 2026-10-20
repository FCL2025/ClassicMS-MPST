//! spritesheet 索引（WZSS）逆向 + spriteset container 路徑偵查。
//!
//! 用法：
//! ```text
//! cargo run --release --example ss_probe -- <bundle 目錄> <spritesheet bundle 名(無副檔名)>
//! ```

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&args[0]);
    let stem = &args[1];
    let needle = args.get(2).map(String::as_str);
    let path = dir.join(format!("{stem}.bundle"));
    let bundle = Bundle::open(&path)?;

    for ni in 0..bundle.nodes.len() {
        if !bundle.nodes[ni].is_serialized_file() {
            continue;
        }
        let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
        let meta_len = metadata_len(&head48)?;
        let head = bundle.read_node_range(ni, 0, meta_len)?;
        let sf = SerializedFile::parse(&head)?;

        // container 全列出
        for o in sf.objects_by_class(objects::CLASS_ASSET_BUNDLE) {
            let data = bundle.read_node_range(
                ni,
                sf.data_offset + o.byte_start as u64,
                o.byte_size as usize,
            )?;
            let ab = objects::parse_asset_bundle(&data, &sf)?;
            println!("== m_Container（{} 筆）==", ab.container.len());
            for e in ab.container.iter() {
                if needle.is_none_or(|needle| e.path.contains(needle)) {
                    println!("  {} -> pid {}", e.path, e.asset.path_id);
                }
            }
        }

        // Texture2D 名稱列表
        println!("\n== Texture2D 名稱 ==");
        for o in sf.objects_by_class(objects::CLASS_TEXTURE2D) {
            let data = bundle.read_node_range(
                ni,
                sf.data_offset + o.byte_start as u64,
                o.byte_size as usize,
            )?;
            let tex = objects::parse_texture2d(&data, &sf)?;
            println!(
                "  pid {} {:?}: {}x{}",
                o.path_id, tex.name, tex.width, tex.height
            );
        }

        // 全部索引 MonoBehaviour 完整 hexdump（最多 3 筆）
        println!("\n== 索引 MonoBehaviour ==");
        let mut dumped = 0;
        for o in sf.objects_by_class(objects::CLASS_MONO_BEHAVIOUR) {
            let data = bundle.read_node_range(
                ni,
                sf.data_offset + o.byte_start as u64,
                o.byte_size as usize,
            )?;
            let mb = objects::parse_mono_behaviour(&data, &sf)?;
            if let Some(needle) = needle {
                if !mb.name.contains(needle) {
                    continue;
                }
            } else if dumped >= 3 {
                break;
            }
            dumped += 1;
            let payload = &data[mb.header_len..];
            println!(
                "--- {:?} (pid {}) payload {} bytes ---",
                mb.name,
                o.path_id,
                payload.len()
            );
            for (row, chunk) in payload.chunks(16).enumerate() {
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
        }
    }
    Ok(())
}
