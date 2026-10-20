//! WZSS hash 假設驗證：section4 的 32-hex 是否為 canvas pool-B 路徑的 MD5。
//! 用法：`cargo run --release --example wzss_hash -- <wzjs bin> <wzss bin>`

use std::collections::{HashMap, HashSet};
use std::hash::Hasher;

use hashers::jenkins::spooky_hash::SpookyHasher;
use msclassic::wzjs::{self, Value, WzNode};

fn collect_canvas(n: &WzNode, out: &mut Vec<String>) {
    if matches!(n.value, Value::Canvas) {
        out.push(n.path.clone().unwrap_or_default());
    }
    for c in &n.children {
        collect_canvas(c, out);
    }
}

fn collect_node_strings(n: &WzNode, out: &mut Vec<String>) {
    out.push(n.name.clone());
    if let Some(path) = &n.path {
        out.push(path.clone());
    }
    for child in &n.children {
        collect_node_strings(child, out);
    }
}

fn read_pool(data: &[u8], magic: usize, count: usize, blob: usize, table: usize) -> Vec<String> {
    let offsets: Vec<usize> = (0..=count)
        .map(|index| {
            u32::from_le_bytes(
                data[magic + table + index * 4..magic + table + index * 4 + 4]
                    .try_into()
                    .unwrap(),
            ) as usize
        })
        .collect();
    offsets
        .windows(2)
        .map(|range| {
            String::from_utf8_lossy(&data[magic + blob + range[0]..magic + blob + range[1]])
                .into_owned()
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash128_byte_formats(bytes: [u8; 16]) -> HashSet<String> {
    let mut out = HashSet::new();
    out.insert(hex(&bytes));

    let mut reversed = bytes;
    reversed.reverse();
    out.insert(hex(&reversed));

    let mut swapped_halves = bytes;
    swapped_halves[..8].copy_from_slice(&bytes[8..]);
    swapped_halves[8..].copy_from_slice(&bytes[..8]);
    out.insert(hex(&swapped_halves));

    let mut reversed_words = bytes;
    for word in reversed_words.as_chunks_mut::<4>().0 {
        word.reverse();
    }
    out.insert(hex(&reversed_words));
    out
}

fn spooky_formats(bytes: &[u8], seed1: u64, seed2: u64) -> HashSet<String> {
    let mut hasher = SpookyHasher::new(seed1, seed2);
    hasher.write(bytes);
    let (a, b) = hasher.finish128();
    let mut memory = Vec::with_capacity(16);
    memory.extend_from_slice(&a.to_le_bytes());
    memory.extend_from_slice(&b.to_le_bytes());
    let mut out = HashSet::new();
    out.insert(format!("{a:016x}{b:016x}"));
    out.insert(format!("{b:016x}{a:016x}"));
    out.insert(hex(&memory));
    memory.reverse();
    out.insert(hex(&memory));
    for swap_halves in [false, true] {
        for reverse_words in [false, true] {
            let words = if swap_halves {
                [b as u32, (b >> 32) as u32, a as u32, (a >> 32) as u32]
            } else {
                [a as u32, (a >> 32) as u32, b as u32, (b >> 32) as u32]
            };
            let mut value = String::new();
            for word in words {
                if reverse_words {
                    value.push_str(&hex(&word.to_le_bytes()));
                } else {
                    value.push_str(&format!("{word:08x}"));
                }
            }
            out.insert(value);
        }
    }
    out
}

fn xxh3_formats(bytes: &[u8], seed: u64) -> HashSet<String> {
    let value = xxhash_rust::xxh3::xxh3_128_with_seed(bytes, seed);
    let mut out = hash128_byte_formats(value.to_le_bytes());
    out.extend(hash128_byte_formats(value.to_be_bytes()));
    out.insert(format!("{value:032x}"));
    out
}

fn path_candidates(root: &str, path: &str) -> Vec<String> {
    let mut values = vec![
        path.to_string(),
        format!("{root}/{path}"),
        format!("{root}.img/{path}"),
        format!("{root}.wz/{path}"),
    ];
    values.extend(
        values
            .clone()
            .into_iter()
            .map(|value| value.to_ascii_lowercase()),
    );
    values.extend(
        values
            .clone()
            .into_iter()
            .map(|value| value.replace('/', "\\")),
    );
    values.sort();
    values.dedup();
    values
}

fn hash128_seeds(hash: &str) -> Vec<(u64, u64)> {
    let bytes: Vec<u8> = hash
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hex"), 16)
                .expect("valid hex")
        })
        .collect();
    let mut values = vec![(0, 0)];
    if bytes.len() == 16 {
        let a_le = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let b_le = u64::from_le_bytes(bytes[8..].try_into().unwrap());
        let a_be = u64::from_be_bytes(bytes[..8].try_into().unwrap());
        let b_be = u64::from_be_bytes(bytes[8..].try_into().unwrap());
        values.extend([(a_le, b_le), (b_le, a_le), (a_be, b_be), (b_be, a_be)]);
    }
    values.sort_unstable();
    values.dedup();
    values
}

fn main() {
    let wzjs_file = std::env::args()
        .nth(1)
        .expect("用法: wzss_hash <wzjs bin> <wzss bin>");
    let wzss_file = std::env::args()
        .nth(2)
        .expect("用法: wzss_hash <wzjs bin> <wzss bin>");

    // WZJS：DFS 收集 canvas 路徑
    let data = std::fs::read(&wzjs_file).unwrap();
    let tree = wzjs::parse(&data).unwrap();
    let mut canvas = Vec::new();
    collect_canvas(&tree, &mut canvas);
    println!("canvas 數：{}", canvas.len());

    let root_path = tree.path.clone().unwrap_or_default();

    // WZSS：32-hex pool A 與 64-hex pool B。
    let d = std::fs::read(&wzss_file).unwrap();
    let magic = d.windows(4).take(1024).position(|x| x == b"WZSS").unwrap();
    let sheet_hash = if magic >= 44 {
        std::str::from_utf8(&d[magic - 44..magic - 12])
            .ok()
            .filter(|value| value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .unwrap_or_default()
    } else {
        ""
    };
    let u32_at = |i: usize| u32::from_le_bytes(d[i * 4..i * 4 + 4].try_into().unwrap());
    let s4 = read_pool(
        &d,
        magic,
        u32_at(13) as usize,
        u32_at(14) as usize,
        u32_at(15) as usize,
    );
    let s5 = read_pool(
        &d,
        magic,
        u32_at(16) as usize,
        u32_at(17) as usize,
        u32_at(18) as usize,
    );
    println!("section4 ({}): {:?}", s4.len(), &s4[..2]);
    println!("section5 ({}): {:?}", s5.len(), &s5[..1]);

    // 假設：md5(canvas path) ∈ section4
    let mut hit = 0;
    for p in &canvas {
        let h = format!("{:x}", md5::compute(p.as_bytes()));
        if s4.iter().any(|x| x == &h) {
            hit += 1;
        } else {
            println!("MISS md5({p}) = {h}");
        }
    }
    println!("md5(pool-B path) 命中 section4：{hit}/{}", canvas.len());

    // 反過來：section4 的 hash 有多少能被 md5(path) 解釋
    let hashes: std::collections::HashSet<String> = canvas
        .iter()
        .map(|p| format!("{:x}", md5::compute(p.as_bytes())))
        .collect();
    let s4_hit = s4.iter().filter(|x| hashes.contains(*x)).count();
    println!("section4 被解釋：{s4_hit}/{}", s4.len());

    // section5 的 64-hex 前 32 碼 vs section4
    let s5a: Vec<&str> = s5.iter().map(|x| &x[..32]).collect();
    let s5b: Vec<&str> = s5.iter().map(|x| &x[32..]).collect();
    let inter_a = s4.iter().filter(|x| s5a.contains(&x.as_str())).count();
    let inter_b = s4.iter().filter(|x| s5b.contains(&x.as_str())).count();
    println!("section4 ∩ section5[..32]：{inter_a}；section4 ∩ section5[32..]：{inter_b}");

    // UnityEngine.Hash128.Compute 使用 SpookyHash128；嘗試常見路徑形式與 UTF-8/UTF-16LE。
    let pool_a: HashSet<&str> = s4.iter().map(String::as_str).collect();
    let mut matches: HashMap<String, Vec<String>> = HashMap::new();
    let mut xxh3_matches: HashMap<String, Vec<String>> = HashMap::new();
    let mut node_strings = Vec::new();
    collect_node_strings(&tree, &mut node_strings);
    node_strings.extend(canvas.iter().cloned());
    node_strings.push(root_path.clone());
    node_strings.sort();
    node_strings.dedup();
    let seeds = hash128_seeds(sheet_hash);
    println!("sheet hash={sheet_hash:?}；seed 形式 {} 種", seeds.len());
    for path in &node_strings {
        let basic = path_candidates(&root_path, path);
        let mut candidates = basic.clone();
        for candidate in basic {
            for separator in ["", "/", ":", "|", "\0"] {
                candidates.push(format!("{sheet_hash}{separator}{candidate}"));
                candidates.push(format!("{candidate}{separator}{sheet_hash}"));
            }
        }
        candidates.sort();
        candidates.dedup();
        for candidate in candidates {
            let mut encodings = vec![("utf8", candidate.as_bytes().to_vec())];
            encodings.push((
                "utf16le",
                candidate
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            ));
            for (encoding, bytes) in encodings {
                for &(seed1, seed2) in &seeds {
                    for hash in spooky_formats(&bytes, seed1, seed2) {
                        if pool_a.contains(hash.as_str()) {
                            matches.entry(hash).or_default().push(format!(
                                "{encoding}:seed={seed1:016x},{seed2:016x}:{candidate}"
                            ));
                        }
                    }
                    for seed in [seed1, seed2] {
                        for hash in xxh3_formats(&bytes, seed) {
                            if pool_a.contains(hash.as_str()) {
                                xxh3_matches
                                    .entry(hash)
                                    .or_default()
                                    .push(format!("{encoding}:seed={seed:016x}:{candidate}"));
                            }
                        }
                    }
                }
            }
        }
    }
    println!(
        "Unity Hash128/SpookyHash 命中：{}/{}",
        matches.len(),
        s4.len()
    );
    for (hash, candidates) in matches {
        println!("  {hash} <- {candidates:?}");
    }
    println!("xxHash3-128 命中：{}/{}", xxh3_matches.len(), s4.len());
    for (hash, candidates) in xxh3_matches {
        println!("  {hash} <- {candidates:?}");
    }
}
