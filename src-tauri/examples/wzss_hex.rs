//! WZSS 區域 hexdump / 字串解讀：針對單一 .bin 檔印出各 section 的原始內容。
//! 用法：`cargo run --release --example wzss_hex -- <bin 檔>`

fn main() {
    let file = std::env::args().nth(1).expect("用法: wzss_hex <bin>");
    let data = std::fs::read(&file).unwrap();
    let magic = data
        .windows(4)
        .take(1024)
        .position(|x| x == b"WZSS")
        .unwrap();
    let u32_at = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
    println!("len={} magic@{magic}", data.len());
    for s in 0..6 {
        let (c, d, t) = (
            u32_at(1 + s * 3) as usize,
            u32_at(2 + s * 3) as usize,
            u32_at(3 + s * 3) as usize,
        );
        println!("\n== section{s}: count={c} dataOff={d} tableOff={t}");
        // data 區域當字串看（前 256 bytes）
        let base = magic + d;
        if base < data.len() {
            let end = (base + 256).min(data.len());
            let ascii: String = data[base..end]
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            println!("  data as ascii: {ascii}");
        }
        // table 區域前 16 個 u32
        let tb = magic + t;
        if tb + 4 <= data.len() {
            let n = ((data.len() - tb) / 4).min(16);
            let v: Vec<u32> = (0..n)
                .map(|k| u32::from_le_bytes(data[tb + k * 4..tb + k * 4 + 4].try_into().unwrap()))
                .collect();
            println!("  table u32[0..{n}]: {v:?}");
        }
    }
}
