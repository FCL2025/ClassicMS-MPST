//! frame_map v2：尺寸分組內窮舉最佳指派，產生高可信 rect→canvas 對應，
//! 並輸出 rect 序的 canvas 排列（給排序規則分析用）。
//!
//! 用法：`cargo run --release --example frame_map2 -- <v83 Character.wz> <wzss bin> <atlas png> [WZ directory] [image id]`

use maple_lib::wz::container::file::WzFile;
use maple_lib::wz::property::WzImageProperty;
use maple_lib::wz::types::WzMapleVersion;
use msclassic::image::parse_wzss;

fn collect(prop: &WzImageProperty, path: &str, out: &mut Vec<(String, image::RgbaImage)>) {
    if let WzImageProperty::Canvas(c) = prop {
        if let Ok(img) = c.png.decompress() {
            out.push((path.to_string(), img));
        }
    }
    if let Some(children) = prop.children() {
        for ch in children {
            collect(ch, &format!("{path}/{}", ch.name()), out);
        }
    }
}

fn diff(a: &image::RgbaImage, b: &image::RgbaImage) -> u64 {
    a.as_raw()
        .iter()
        .zip(b.as_raw().iter())
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wz_directory = args.get(3).map_or("Face", String::as_str);
    let image_id = args.get(4).map_or("00020116", String::as_str);
    let mut f = WzFile::open(args[0].clone(), WzMapleVersion::Gms);
    f.parse_wz_file().unwrap();
    let root = f.wz_directory_mut().unwrap();
    let img = root
        .get_directory_mut(wz_directory)
        .and_then(|d| d.get_image_mut(&format!("{image_id}.img")))
        .unwrap_or_else(|| panic!("{wz_directory}/{image_id}.img 不存在"));
    img.parse_image().unwrap();
    let mut v83: Vec<(String, image::RgbaImage)> = Vec::new();
    for p in &img.properties {
        collect(p, p.name(), &mut v83);
    }

    let wzss = parse_wzss(&std::fs::read(&args[1]).unwrap()).unwrap();
    let atlas = image::open(&args[2]).unwrap().to_rgba8();
    let (_, ah) = atlas.dimensions();

    // rect 切圖
    let cuts: Vec<image::RgbaImage> = wzss
        .rects
        .iter()
        .map(|r| image::imageops::crop_imm(&atlas, r.x, ah - r.y - r.h, r.w, r.h).to_image())
        .collect();

    // 依尺寸分組（v83 與 rect 同尺寸才配對）
    let mut assigned: Vec<Option<usize>> = vec![None; cuts.len()]; // rect_j -> v83 index
    let mut used = vec![false; v83.len()];
    // 收集所有尺寸
    let mut sizes: Vec<(u32, u32)> = v83.iter().map(|(_, im)| im.dimensions()).collect();
    sizes.extend(cuts.iter().map(|c| c.dimensions()));
    sizes.sort();
    sizes.dedup();
    for sz in sizes {
        let ridx: Vec<usize> = (0..cuts.len())
            .filter(|&j| cuts[j].dimensions() == sz)
            .collect();
        let vidx: Vec<usize> = (0..v83.len())
            .filter(|&i| v83[i].1.dimensions() == sz)
            .collect();
        if ridx.is_empty() || vidx.is_empty() {
            continue;
        }
        // 窮舉指派（group 很小）：把 ridx 配給 vidx 的子集，min total diff
        let mut best: Option<(u64, Vec<(usize, usize)>)> = None;
        // 產生 vidx 的排列中取 ridx.len() 個
        let n = ridx.len();
        let mut perm: Vec<usize> = (0..vidx.len()).collect();
        fn permutations(v: &mut [usize], l: usize, k: usize, f: &mut dyn FnMut(&[usize])) {
            if l == k {
                f(&v[..k]);
                return;
            }
            for i in l..v.len() {
                v.swap(l, i);
                permutations(v, l + 1, k, f);
                v.swap(l, i);
            }
        }
        let mut consider = |sel: &[usize]| {
            // sel[k] = 配給 ridx[k] 的 vidx 位置
            let total: u64 = sel
                .iter()
                .enumerate()
                .map(|(k, &vi)| diff(&cuts[ridx[k]], &v83[vidx[vi]].1))
                .sum();
            if best.as_ref().is_none_or(|(d, _)| total < *d) {
                best = Some((
                    total,
                    sel.iter()
                        .enumerate()
                        .map(|(k, &vi)| (ridx[k], vidx[vi]))
                        .collect(),
                ));
            }
        };
        permutations(&mut perm, 0, n, &mut |sel| consider(sel));
        if let Some((_, pairs)) = best {
            for (rj, vi) in pairs {
                assigned[rj] = Some(vi);
                used[vi] = true;
            }
        }
    }

    println!("rect 序 → canvas（指派結果）：");
    for (j, a) in assigned.iter().enumerate() {
        let r = &wzss.rects[j];
        match a {
            Some(vi) => {
                let d = diff(&cuts[j], &v83[*vi].1);
                println!(
                    "  rect[{j:2}] {:2}x{:<2} → {} (diff={d})",
                    r.w, r.h, v83[*vi].0
                );
            }
            None => println!("  rect[{j:2}] {:2}x{:<2} → ?", r.w, r.h),
        }
    }
    let unmatched: Vec<&str> = v83
        .iter()
        .enumerate()
        .filter(|(i, _)| !used[*i])
        .map(|(_, (p, _))| p.as_str())
        .collect();
    println!("\n無 rect 的 canvas：{unmatched:?}");

    // 輸出 rect 序 canvas 名（給 python 分析排序規則）
    let seq: Vec<String> = assigned
        .iter()
        .map(|a| a.map(|vi| v83[vi].0.clone()).unwrap_or_else(|| "?".into()))
        .collect();
    println!("\nSEQ:{}", seq.join(","));
}
