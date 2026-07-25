//! 偵錯用：列出指定 bundle 內 MonoBehaviour 的 m_Name 重名情形（含大小寫不敏感）。

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args().nth(1).unwrap());
    let bundle = Bundle::open(&path)?;
    let ni = bundle
        .nodes
        .iter()
        .position(|n| n.is_serialized_file())
        .unwrap();
    let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
    let meta_len = metadata_len(&head48)?;
    let head = bundle.read_node_range(ni, 0, meta_len)?;
    let sf = SerializedFile::parse(&head)?;

    let mut names: Vec<(String, i64)> = Vec::new();
    for o in sf.objects_by_class(objects::CLASS_MONO_BEHAVIOUR) {
        let data = bundle.read_node_range(
            ni,
            sf.data_offset + o.byte_start as u64,
            o.byte_size as usize,
        )?;
        let mb = objects::parse_mono_behaviour(&data, &sf)?;
        names.push((mb.name, o.path_id));
    }
    let total = names.len();
    names.sort_by_key(|entry| entry.0.to_lowercase());
    let mut dup = 0;
    for w in names.windows(2) {
        if w[0].0.to_lowercase() == w[1].0.to_lowercase() {
            dup += 1;
            println!(
                "重名: {:?} (pid {}) == {:?} (pid {})",
                w[0].0, w[0].1, w[1].0, w[1].1
            );
        }
    }
    println!("total {total}, 大小寫不敏感重名 {dup}");
    Ok(())
}
