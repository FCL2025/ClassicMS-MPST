//! 一次性探測：dump sound bundle 的 node 名稱 + 第一個 AudioClip 的原始 bytes。
//! 用法: cargo run --release --example probe_audio -- <sound bundle 路徑>

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("需要 bundle 路徑");
    let bundle = Bundle::open(path.as_ref())?;
    println!("nodes:");
    for (i, n) in bundle.nodes.iter().enumerate() {
        println!(
            "  [{i}] {:?} (serialized={})",
            n.name,
            n.is_serialized_file()
        );
    }
    let ni = bundle
        .nodes
        .iter()
        .position(|n| n.is_serialized_file())
        .unwrap();
    let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
    let meta_len = metadata_len(&head48)?;
    let head = bundle.read_node_range(ni, 0, meta_len)?;
    let sf = SerializedFile::parse(&head)?;

    let clips: Vec<_> = sf.objects_by_class(objects::CLASS_AUDIO_CLIP).collect();
    println!("AudioClip 數量: {}", clips.len());

    // container 路徑前綴分佈（查哪些沒被 normalize 收到）
    {
        use std::collections::HashMap;
        let by_path_id: HashMap<i64, &msclassic::unity::serialized_file::ObjectInfo> =
            sf.objects.iter().map(|o| (o.path_id, o)).collect();
        for o in sf.objects_by_class(objects::CLASS_ASSET_BUNDLE) {
            let data = bundle.read_node_range(
                ni,
                sf.data_offset + o.byte_start as u64,
                o.byte_size as usize,
            )?;
            let ab = objects::parse_asset_bundle(&data, &sf)?;
            let mut prefixes: HashMap<String, usize> = HashMap::new();
            let mut missing_pid = 0usize;
            let mut ext_ref = 0usize;
            for e in &ab.container {
                let key: String = e
                    .path
                    .split('/')
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("/")
                    .to_lowercase();
                *prefixes.entry(key).or_default() += 1;
                if e.asset.file_id != 0 {
                    ext_ref += 1;
                } else if !by_path_id.contains_key(&e.asset.path_id) {
                    missing_pid += 1;
                }
            }
            let mut prefixes: Vec<_> = prefixes.into_iter().collect();
            prefixes.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            println!(
                "container {} 筆（外部參照 {ext_ref}、path_id 查無 {missing_pid}），前綴分佈:",
                ab.container.len()
            );
            for (p, n) in prefixes.iter().take(12) {
                println!("  {n:6}  {p}");
            }
            let sample: Vec<&str> = ab
                .container
                .iter()
                .take(2)
                .map(|e| e.path.as_str())
                .collect();
            println!("  樣本: {sample:?}");
            // 副檔名 + 第 4 段分佈
            let mut exts: HashMap<String, usize> = HashMap::new();
            let mut seg4: HashMap<String, usize> = HashMap::new();
            for e in &ab.container {
                let lower = e.path.to_lowercase();
                let ext = lower.rsplit('.').next().unwrap_or("").to_string();
                *exts.entry(ext).or_default() += 1;
                let s4 = lower.split('/').nth(3).unwrap_or("").to_string();
                *seg4.entry(s4).or_default() += 1;
            }
            let mut exts: Vec<_> = exts.into_iter().collect();
            exts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            println!("  副檔名: {exts:?}");
            let mut seg4: Vec<_> = seg4.into_iter().collect();
            seg4.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            println!("  第 4 段前 15: {:?}", &seg4[..seg4.len().min(15)]);
        }
    }
    for o in clips.iter().take(3) {
        let data = bundle.read_node_range(
            ni,
            sf.data_offset + o.byte_start as u64,
            o.byte_size as usize,
        )?;
        println!(
            "\npath_id={} byte_size={} 前 192 bytes:",
            o.path_id, o.byte_size
        );
        for (row, chunk) in data[..data.len().min(192)].chunks(16).enumerate() {
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
            println!("  {:04x}  {:47}  {ascii}", row * 16, hex.join(" "));
        }
    }
    // 驗證萃取：照推測佈局解析前 5 個，從 resource node 讀出聲音資料看 mp3 magic
    let res_node = 1usize;
    // 掃描全部 clip 的 FSB sample header，收集 VORBISDATA chunk 的 crc32
    {
        let mut crcs = std::collections::HashSet::new();
        let mut codecs = std::collections::HashMap::new();
        for o in &clips {
            let data = bundle.read_node_range(
                ni,
                sf.data_offset + o.byte_start as u64,
                o.byte_size as usize,
            )?;
            let mut r = msclassic::unity::Reader::new(&data, true);
            let _name = r.aligned_string()?;
            r.skip(4 * 4 + 4)?; // load/ch/freq/bits/length
            let _tracker = r.bool()?;
            r.align4();
            r.skip(4)?; // subsound
            r.skip(3)?; // 3 bools
            r.align4();
            let source = r.aligned_string()?;
            let offset = r.u64()?;
            let size = r.u64()?;
            let node_name = source.rsplit('/').next().unwrap_or(&source);
            debug_assert_eq!(node_name, "CAB-ac82b40e5bd6406ac1df153960198747.resource");
            // 只讀 FSB 前 4KB 就夠涵蓋 sample header（BGM 的 seek 表可能超過 160）
            let head = bundle.read_node_range(res_node, offset, 4096.min(size as usize))?;
            if &head[..4] != b"FSB5" {
                continue;
            }
            let i32at = |p: usize| i32::from_le_bytes(head[p..p + 4].try_into().unwrap());
            let sh_size = i32at(12) as usize;
            let codec = i32at(24) & 0x1f;
            *codecs.entry(codec).or_insert(0usize) += 1;
            if sh_size + 60 > head.len() {
                continue;
            }
            let sh = &head[60..60 + sh_size];
            if sh.len() < 8 {
                continue;
            }
            let meta = u64::from_le_bytes(sh[..8].try_into().unwrap());
            let has_chunks = meta & 1 == 1;
            if !has_chunks {
                continue;
            }
            // 走 chunk 鏈找 VORBISDATA(type 11)
            let mut p = 8usize;
            loop {
                if p + 4 > sh.len() {
                    break;
                }
                let info = u32::from_le_bytes(sh[p..p + 4].try_into().unwrap());
                let more = info & 1 == 1;
                let csize = ((info >> 1) & 0xFF_FFFF) as usize;
                let ctype = (info >> 25) & 0x7F;
                if ctype == 11 && p + 4 + 4 <= sh.len() {
                    let crc = u32::from_le_bytes(sh[p + 4..p + 8].try_into().unwrap());
                    crcs.insert(crc);
                }
                p += 4 + csize;
                if !more {
                    break;
                }
            }
        }
        println!(
            "\n== 全掃描: clips={} codecs={codecs:?} 相異 VORBISDATA crc32 = {} 個 ==",
            clips.len(),
            crcs.len()
        );
        for c in &crcs {
            println!("  {c:#010x}");
        }
    }
    for o in clips.iter().take(5) {
        let data = bundle.read_node_range(
            ni,
            sf.data_offset + o.byte_start as u64,
            o.byte_size as usize,
        )?;
        let mut r = msclassic::unity::Reader::new(&data, true);
        let name = r.aligned_string()?;
        let load_type = r.i32()?;
        let channels = r.i32()?;
        let freq = r.i32()?;
        let bits = r.i32()?;
        let length = r.f32()?;
        let _tracker = r.bool()?;
        r.align4();
        let subsound = r.i32()?;
        let preload = r.bool()?;
        let bg = r.bool()?;
        let legacy3d = r.bool()?;
        r.align4();
        let source = r.aligned_string()?;
        let offset = r.u64()?;
        let size = r.u64()?;
        let node_name = source.rsplit('/').next().unwrap_or(&source);
        let audio = bundle.read_node_range(res_node, offset, size as usize)?;
        let magic = &audio[..audio.len().min(4)];
        println!(
            "  {name:?} load={load_type} ch={channels} freq={freq} bits={bits} len={length:.2}s sub={subsound} pre={preload} bg={bg} 3d={legacy3d}\n    node={node_name:?} off={offset} size={size} magic={magic:02x?}"
        );
        // FSB5 header 解析（固定 60 bytes header + sample headers + name table + data）
        if magic == b"FSB5" {
            let i32at = |p: usize| i32::from_le_bytes(audio[p..p + 4].try_into().unwrap());
            let num_samples = i32at(8);
            let sh_size = i32at(12);
            let name_size = i32at(16);
            let data_size = i32at(20);
            let mode = u32::from_le_bytes(audio[24..28].try_into().unwrap());
            let codec = mode & 0x1f;
            let data_off = 60 + sh_size as usize + name_size as usize;
            let sh: Vec<String> = audio[60..(60 + sh_size as usize).min(audio.len())]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            println!(
                "    FSB5: samples={num_samples} shSize={sh_size} nameSize={name_size} dataSize={data_size} codec={codec} dataOff={data_off}\n    sampleHdr: {}",
                sh.join(" ")
            );
            // 假設：data 區 = 每包 u16(LE) 長度前綴 + Opus packet，走訪驗證是否正好走完
            let data = &audio[data_off..(data_off + data_size as usize).min(audio.len())];
            let mut pos = 0usize;
            let mut packets = 0usize;
            let mut tocs = Vec::new();
            while pos + 2 <= data.len() {
                let plen = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                pos += 2;
                if pos + plen > data.len() {
                    break;
                }
                if packets < 3 {
                    tocs.push(format!("{:#04x}", data[pos]));
                }
                pos += plen;
                packets += 1;
            }
            println!(
                "    u16-walk: packets={packets} 剩餘={} tail={} 首包TOC={tocs:?}",
                data.len() - pos,
                if data.len() - pos <= 4 { "ok" } else { "BAD" }
            );
            // 第 2 個參數：把這個 clip 的 FSB5 原樣輸出
            if let Some(out) = std::env::args().nth(2) {
                if clips.first().map(|f| f.path_id) == Some(o.path_id) {
                    std::fs::write(&out, &audio)?;
                    println!("    dumped {} bytes → {out}", audio.len());
                }
            }
        }
    }
    // 第 3 個參數：dump 最大的那個 clip（BGM 等長音檔驗證用）
    if let Some(out) = std::env::args().nth(3) {
        let big = clips.iter().max_by_key(|o| o.byte_size).unwrap();
        let data = bundle.read_node_range(
            ni,
            sf.data_offset + big.byte_start as u64,
            big.byte_size as usize,
        )?;
        let mut r = msclassic::unity::Reader::new(&data, true);
        let name = r.aligned_string()?;
        r.skip(4 * 5)?;
        let _ = r.bool()?;
        r.align4();
        r.skip(4)?;
        r.skip(3)?;
        r.align4();
        let _source = r.aligned_string()?;
        let offset = r.u64()?;
        let size = r.u64()?;
        let audio = bundle.read_node_range(res_node, offset, size as usize)?;
        std::fs::write(&out, &audio)?;
        println!("最大 clip {name:?}: FSB {size} bytes → {out}");
    }
    Ok(())
}
