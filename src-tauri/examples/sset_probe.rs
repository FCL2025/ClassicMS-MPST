//! spriteset 偵查：container 路徑、Sprite(213) 名稱/rect、Texture2D 名稱對照。
//!
//! 用法：
//! ```text
//! cargo run --release --example sset_probe -- <bundle 目錄>
//! ```

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
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("spriteset_")
        })
        .collect();
    files.sort();

    for path in &files {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let bundle = Bundle::open(path)?;
        for ni in 0..bundle.nodes.len() {
            if !bundle.nodes[ni].is_serialized_file() {
                continue;
            }
            let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
            let meta_len = metadata_len(&head48)?;
            let head = bundle.read_node_range(ni, 0, meta_len)?;
            let sf = SerializedFile::parse(&head)?;

            for o in sf.objects_by_class(objects::CLASS_ASSET_BUNDLE) {
                let data = bundle.read_node_range(
                    ni,
                    sf.data_offset + o.byte_start as u64,
                    o.byte_size as usize,
                )?;
                let ab = objects::parse_asset_bundle(&data, &sf)?;
                println!("[{stem}] container {} 筆:", ab.container.len());
                for e in ab.container.iter() {
                    println!("   {} -> pid {}", e.path, e.asset.path_id);
                }
            }

            println!("[{stem}] Sprite(213) 前 8:");
            for (i, o) in sf.objects_by_class(objects::CLASS_SPRITE).enumerate() {
                if i >= 8 {
                    break;
                }
                let data = bundle.read_node_range(
                    ni,
                    sf.data_offset + o.byte_start as u64,
                    o.byte_size as usize,
                )?;
                let sp = objects::parse_sprite(&data, &sf)?;
                println!(
                    "   {:?} rect=({:.0},{:.0},{:.0}x{:.0})",
                    sp.name, sp.rect.0, sp.rect.1, sp.rect.2, sp.rect.3
                );
            }

            println!("[{stem}] Texture2D 前 8:");
            for (i, o) in sf.objects_by_class(objects::CLASS_TEXTURE2D).enumerate() {
                if i >= 8 {
                    break;
                }
                let data = bundle.read_node_range(
                    ni,
                    sf.data_offset + o.byte_start as u64,
                    o.byte_size as usize,
                )?;
                let tex = objects::parse_texture2d(&data, &sf)?;
                println!(
                    "   pid {} {:?}: {}x{} inline={} stream=({},{})",
                    o.path_id,
                    tex.name,
                    tex.width,
                    tex.height,
                    tex.image_data_size,
                    tex.stream_offset,
                    tex.stream_size
                );
            }
        }
    }
    Ok(())
}
