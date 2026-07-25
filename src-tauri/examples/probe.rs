//! 偵錯用：dump 指定 bundle 第一個 SerializedFile node 的前 N bytes。

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = PathBuf::from(&args[0]);
    let len: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(128);
    let bundle = Bundle::open(&path)?;
    for (i, n) in bundle.nodes.iter().enumerate() {
        println!(
            "node {i}: {:?} offset={} size={} flags={:#x}",
            n.name, n.offset, n.size, n.flags
        );
    }
    let ni = bundle
        .nodes
        .iter()
        .position(|n| n.is_serialized_file())
        .expect("no serialized node");
    let data = bundle.read_node_range(ni, 0, len as u64 as usize)?;
    for (row, chunk) in data.chunks(16).enumerate() {
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
        println!("{:06x}: {:<47}  {}", row * 16, hex.join(" "), ascii);
    }
    Ok(())
}
