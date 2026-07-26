//! 暫時偵查用：列出 prefab bundle container 路徑（可用引數過濾關鍵字）。

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = PathBuf::from(&args[0]);
    let filter = args.get(1).map(|s| s.to_lowercase());
    let bundle = Bundle::open(&path)?;
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
            println!("container {} 筆", ab.container.len());
            for e in ab.container.iter() {
                if let Some(f) = &filter {
                    if !e.path.to_lowercase().contains(f) {
                        continue;
                    }
                }
                println!("   {} -> pid {}", e.path, e.asset.path_id);
            }
        }
    }
    Ok(())
}
