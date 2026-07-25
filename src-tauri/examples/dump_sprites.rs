//! spriteset → PNG 全量輸出（驗收 a：解碼正確性肉眼抽查）。
//!
//! 用法：
//! ```text
//! cargo run --release --example dump_sprites -- <bundle 目錄> <輸出目錄>
//! ```

use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};
use msclassic::unity::texture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&args[0]);
    let out_root = PathBuf::from(&args[1]);

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

    let t0 = std::time::Instant::now();
    let mut ok = 0usize;
    let mut fail = 0usize;
    for path in &files {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let out_dir = out_root.join(&stem);
        std::fs::create_dir_all(&out_dir)?;
        let bundle = Bundle::open(path)?;
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
                let tex = match objects::parse_texture2d(&data, &sf) {
                    Ok(t) => t,
                    Err(e) => {
                        println!("!! [{stem}] (pid {}): parse: {e}", o.path_id);
                        fail += 1;
                        continue;
                    }
                };
                let result = texture::fetch_pixels_flipped(&bundle, ni, &tex, &data)
                    .and_then(|rgba| texture::to_png(tex.width as u32, tex.height as u32, &rgba));
                match result {
                    Ok(png) => {
                        let name: String = tex
                            .name
                            .chars()
                            .map(|c| {
                                if "<>:\"/\\|?*".contains(c) || c.is_control() {
                                    '_'
                                } else {
                                    c
                                }
                            })
                            .collect();
                        let fname = format!("{}_{}.png", name, o.path_id);
                        std::fs::write(out_dir.join(fname), png)?;
                        ok += 1;
                    }
                    Err(e) => {
                        println!("!! [{stem}] {:?} (pid {}): {e}", tex.name, o.path_id);
                        fail += 1;
                    }
                }
            }
        }
        println!("[{stem}] 完成（累計 ok={ok} fail={fail}）");
    }
    println!(
        "\n=== 全部完成：ok={ok} fail={fail}，耗時 {:.2?} ===",
        t0.elapsed()
    );
    Ok(())
}
