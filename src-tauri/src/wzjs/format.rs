//! WZJS v5 解碼器（逆向定案）。
//!
//! 輸入：MonoBehaviour raw payload bytes（Phase 1 dump 出來的 .bin）。
//! 輸出：[`WzNode`] 節點樹。
//!
//! ## 檔案佈局（全部 little-endian）
//!
//! ```text
//! [0..4)      u32 nodeCount
//! [4..8)      u32 常數 8（用途不明，全部樣本皆為 8）
//! [8..144)    12 個 section 描述（各 count u32 + offset u32）：
//!             slot0 = child order table（u32 陣列）
//!             slot1 = type 3 的值（u8 陣列）
//!             slot3 = type 5 的值（u16 陣列）
//!             slot4 = type 6 的值（i32 陣列）
//!             slot5 = type 7 的值（i64 陣列）
//!             slot6 = type 8 的值（8 bytes/筆 = 2×f32）
//!             slot7 = type 14 的值（8 bytes/筆 = vector 2×f32）
//!             slot8 = type 9 的值（f64 陣列）
//!             slot 2/9/10/11 全部樣本皆為空
//! [144..148)  magic "WZJS"
//! [148..152)  u32 version = 5
//! [152..)     節點記錄 nodeCount × 32 bytes（8 × i32，見下）
//!             之後依序是各 section 的資料、child order table、字串池 A/B/C
//!             （所有 offset 一律以 magic 位置 144 為基準）
//! ```
//!
//! header 尾段（idx26..35）：
//! ```text
//! idx26..28  字串池 A（節點名稱）：count / blobOffset / tableOffset
//! idx29..31  字串池 B（節點路徑）：同上
//! idx32..34  字串池 C（type 12 字串值）：同上
//! idx35      payload 總長 - 144
//! ```
//!
//! 字串池 = 「資料 blob + (count+1) 筆 u32 offset 表」，第 i 字串 = blob[off[i]..off[i+1]]。
//!
//! ## 節點記錄（32 bytes = 8 × i32）
//!
//! ```text
//! f0  type tag（2/3/5/6/8/9/11/12/14/18/19，對應值型別見 node.rs）
//! f1  名稱 index → 字串池 A
//! f2  值（依 type：section 陣列 index / inline int / pool C index / -1）
//! f3  子節點 node index 連續區間的起點（子節點集合 = [f3, f3+f4)）
//! f4  子節點數量
//! f5  父節點 node index（-1 = 根）
//! f6  路徑 index → 字串池 B（根節點是資產路徑，其餘是 img 內相對路徑）
//! f7  child order table 起點：子節點的「順序」= table[f7 .. f7+f4]
//! ```
//!
//! 設計重點：子節點的 node index 佔連續區間 [f3, f3+f4)，
//! child order table 給出顯示順序（table 內的值都落在該區間內）。

use thiserror::Error;

use super::node::{Value, WzNode};

/// magic 在 payload 內的固定位置。
pub const MAGIC_OFFSET: usize = 144;
/// header u32 數。
pub const HEADER_U32_COUNT: usize = 36;
/// 節點記錄大小。
pub const RECORD_SIZE: usize = 32;

#[derive(Error, Debug)]
pub enum WzjsError {
    #[error("資料太短：{0} bytes（至少需要 152）")]
    TooShort(usize),

    #[error("magic 不符：offset 144 處為 {0:?}（預期 WZJS）")]
    BadMagic(String),

    #[error("不支援的 version: {0}")]
    UnsupportedVersion(u32),

    #[error("資料截斷：需要 {need} bytes @ {pos}，檔案只有 {len} bytes")]
    Truncated { pos: usize, need: usize, len: usize },

    #[error("未知的 type tag: {0}（node {1}）")]
    UnknownType(i32, u32),

    #[error("index 越界：{0}")]
    IndexOutOfRange(String),

    #[error("格式錯誤: {0}")]
    Format(String),
}

type Result<T> = std::result::Result<T, WzjsError>;

fn u32_at(data: &[u8], pos: usize) -> Result<u32> {
    let end = pos
        .checked_add(4)
        .ok_or_else(|| WzjsError::Format("u32 offset 溢位".into()))?;
    let b = data.get(pos..end).ok_or(WzjsError::Truncated {
        pos,
        need: 4,
        len: data.len(),
    })?;
    Ok(u32::from_le_bytes(b.try_into().unwrap()))
}

/// 字串池：blob + (count+1) 筆 offset 表。
fn read_pool(data: &[u8], count: u32, blob_off: u32, table_off: u32) -> Result<Vec<String>> {
    let count = count as usize;
    let offsets_count = count
        .checked_add(1)
        .ok_or_else(|| WzjsError::Format("字串池 count 溢位".into()))?;
    let table_bytes = offsets_count
        .checked_mul(4)
        .ok_or_else(|| WzjsError::Format("字串池 table 大小溢位".into()))?;
    if table_bytes > data.len() {
        return Err(WzjsError::Format(format!(
            "字串池 count 過大: {count}（payload {} bytes）",
            data.len()
        )));
    }
    let blob_pos = MAGIC_OFFSET
        .checked_add(blob_off as usize)
        .ok_or_else(|| WzjsError::Format("字串池 blob offset 溢位".into()))?;
    let table_pos = MAGIC_OFFSET
        .checked_add(table_off as usize)
        .ok_or_else(|| WzjsError::Format("字串池 table offset 溢位".into()))?;
    let table_end = table_pos
        .checked_add(table_bytes)
        .ok_or_else(|| WzjsError::Format("字串池 table range 溢位".into()))?;
    if blob_pos > data.len() || table_end > data.len() {
        return Err(WzjsError::Truncated {
            pos: table_pos.min(blob_pos),
            need: table_bytes,
            len: data.len(),
        });
    }
    let mut offs = Vec::with_capacity(offsets_count);
    for i in 0..offsets_count {
        let relative = i
            .checked_mul(4)
            .ok_or_else(|| WzjsError::Format("字串池 table index 溢位".into()))?;
        let pos = table_pos
            .checked_add(relative)
            .ok_or_else(|| WzjsError::Format("字串池 table index 溢位".into()))?;
        offs.push(u32_at(data, pos)? as usize);
    }
    let mut out = Vec::with_capacity(count);
    for w in offs.windows(2) {
        let (s, e) = (w[0], w[1]);
        if s > e {
            return Err(WzjsError::Format(format!("字串池 offset 倒序: {s} > {e}")));
        }
        let start = blob_pos
            .checked_add(s)
            .ok_or_else(|| WzjsError::Format("字串池字串起點溢位".into()))?;
        let end = blob_pos
            .checked_add(e)
            .ok_or_else(|| WzjsError::Format("字串池字串終點溢位".into()))?;
        let bytes = data.get(start..end).ok_or(WzjsError::Truncated {
            pos: start,
            need: e - s,
            len: data.len(),
        })?;
        out.push(String::from_utf8_lossy(bytes).into_owned());
    }
    Ok(out)
}

/// type 11/12 的 pool C 字串讀取。
fn read_pool_c(idx: i32, node: usize, pool_c: &[String]) -> Result<String> {
    if idx < 0 {
        return Err(WzjsError::Format(format!(
            "node {node} pool C index 為負: {idx}"
        )));
    }
    pool_c.get(idx as usize).cloned().ok_or_else(|| {
        WzjsError::IndexOutOfRange(format!(
            "node {node} pool C index {idx} 越界（pool C {} 筆）",
            pool_c.len()
        ))
    })
}

/// 解析整個 payload，回傳根節點（整棵樹）。
pub fn parse(data: &[u8]) -> Result<WzNode> {
    if data.len() < 152 {
        return Err(WzjsError::TooShort(data.len()));
    }
    if &data[MAGIC_OFFSET..MAGIC_OFFSET + 4] != b"WZJS" {
        return Err(WzjsError::BadMagic(
            String::from_utf8_lossy(&data[MAGIC_OFFSET..MAGIC_OFFSET + 4]).into_owned(),
        ));
    }
    let version = u32_at(data, 148)?;
    if version != 5 {
        return Err(WzjsError::UnsupportedVersion(version));
    }

    // header 36 × u32
    let mut header = [0u32; HEADER_U32_COUNT];
    for (i, h) in header.iter_mut().enumerate() {
        *h = u32_at(data, i * 4)?;
    }
    if header[1] != 8 {
        return Err(WzjsError::Format(format!(
            "header idx1 常數異常: {}（預期 8）",
            header[1]
        )));
    }
    let node_count = header[0] as usize;
    if node_count == 0 {
        return Err(WzjsError::Format("nodeCount 不得為 0".into()));
    }
    let record_bytes = node_count
        .checked_mul(RECORD_SIZE)
        .ok_or_else(|| WzjsError::Format("node records 大小溢位".into()))?;
    if record_bytes > data.len().saturating_sub(152) {
        return Err(WzjsError::Truncated {
            pos: 152,
            need: record_bytes,
            len: data.len(),
        });
    }

    // 12 個 section slot：(count, offset) = (idx2, idx3), (idx4, idx5), ...
    let sec_counts: [usize; 12] = std::array::from_fn(|s| header[2 + s * 2] as usize);
    let mut sec_offsets = [0usize; 12];
    for (s, offset) in sec_offsets.iter_mut().enumerate() {
        *offset = MAGIC_OFFSET
            .checked_add(header[3 + s * 2] as usize)
            .ok_or_else(|| WzjsError::Format(format!("slot{s} offset 溢位")))?;
    }
    let sec_count = |s: usize| sec_counts[s];
    let sec_off = |s: usize| sec_offsets[s];
    // 未使用的 slot 必須為空
    for s in [2usize, 9, 10, 11] {
        if sec_count(s) != 0 {
            return Err(WzjsError::Format(format!(
                "slot{s} 應為空但 count = {}",
                sec_count(s)
            )));
        }
    }
    if (header[35] as usize).checked_add(MAGIC_OFFSET) != Some(data.len()) {
        return Err(WzjsError::Format(format!(
            "payload 長度不符：idx35={} + 144 != {}",
            header[35],
            data.len()
        )));
    }
    for (slot, entry_size) in [
        (0usize, 4usize),
        (1, 1),
        (3, 2),
        (4, 4),
        (5, 8),
        (6, 8),
        (7, 8),
        (8, 8),
    ] {
        let bytes = sec_count(slot)
            .checked_mul(entry_size)
            .ok_or_else(|| WzjsError::Format(format!("slot{slot} 大小溢位")))?;
        let end = sec_off(slot)
            .checked_add(bytes)
            .ok_or_else(|| WzjsError::Format(format!("slot{slot} range 溢位")))?;
        if end > data.len() {
            return Err(WzjsError::Truncated {
                pos: sec_off(slot),
                need: bytes,
                len: data.len(),
            });
        }
    }

    // ── 字串池 ──
    let pool_a = read_pool(data, header[26], header[27], header[28])?;
    let pool_b = read_pool(data, header[29], header[30], header[31])?;
    let pool_c = read_pool(data, header[32], header[33], header[34])?;

    // ── child order table（slot0）──
    let child_table: Vec<u32> = (0..sec_count(0))
        .map(|i| {
            let relative = i
                .checked_mul(4)
                .ok_or_else(|| WzjsError::Format("child table index 溢位".into()))?;
            sec_off(0)
                .checked_add(relative)
                .ok_or_else(|| WzjsError::Format("child table offset 溢位".into()))
                .and_then(|pos| u32_at(data, pos))
        })
        .collect::<Result<_>>()?;

    // ── 節點記錄 ──
    let rec_end = 152usize
        .checked_add(record_bytes)
        .ok_or_else(|| WzjsError::Format("node records 終點溢位".into()))?;
    if rec_end > data.len() {
        return Err(WzjsError::Truncated {
            pos: 152,
            need: record_bytes,
            len: data.len(),
        });
    }
    let rec = |i: usize, f: usize| -> i32 {
        i32::from_le_bytes(
            data[152 + i * RECORD_SIZE + f * 4..][..4]
                .try_into()
                .unwrap(),
        )
    };

    // section 讀取器（entry 大小依 slot）
    let sec_u32 = |slot: usize, idx: i32| -> Result<u32> {
        if idx < 0 || idx as usize >= sec_count(slot) {
            return Err(WzjsError::IndexOutOfRange(format!(
                "slot{slot} index {idx} 越界（count {}）",
                sec_count(slot)
            )));
        }
        let relative = (idx as usize)
            .checked_mul(4)
            .ok_or_else(|| WzjsError::Format(format!("slot{slot} index 溢位")))?;
        let pos = sec_off(slot)
            .checked_add(relative)
            .ok_or_else(|| WzjsError::Format(format!("slot{slot} offset 溢位")))?;
        u32_at(data, pos)
    };
    let sec_u32x2 = |slot: usize, idx: i32| -> Result<(u32, u32)> {
        if idx < 0 || idx as usize >= sec_count(slot) {
            return Err(WzjsError::IndexOutOfRange(format!(
                "slot{slot} pair index {idx} 越界（count {}）",
                sec_count(slot)
            )));
        }
        let relative = (idx as usize)
            .checked_mul(8)
            .ok_or_else(|| WzjsError::Format(format!("slot{slot} pair index 溢位")))?;
        let pos = sec_off(slot)
            .checked_add(relative)
            .ok_or_else(|| WzjsError::Format(format!("slot{slot} pair offset 溢位")))?;
        Ok((u32_at(data, pos)?, u32_at(data, pos + 4)?))
    };

    // ── 子節點 index 必須大於父節點（倒序組樹的前提），先驗證 ──
    for i in 0..node_count {
        let nc = rec(i, 4);
        if nc < 0 {
            return Err(WzjsError::Format(format!("node {i} childCount 為負: {nc}")));
        }
        let nc = nc as usize;
        if nc == 0 {
            continue;
        }
        let ct = rec(i, 7);
        let child_end = if ct < 0 {
            None
        } else {
            (ct as usize).checked_add(nc)
        };
        if child_end.is_none_or(|end| end > child_table.len()) {
            return Err(WzjsError::IndexOutOfRange(format!(
                "node {i} child table [{ct}..+{nc}) 越界（表長 {}）",
                child_table.len()
            )));
        }
        for k in 0..nc {
            let c = child_table[ct as usize + k] as usize;
            if c >= node_count {
                return Err(WzjsError::IndexOutOfRange(format!(
                    "node {i} 的子節點 index {c} 越界（共 {node_count}）"
                )));
            }
            if c <= i {
                return Err(WzjsError::Format(format!(
                    "node {i} 的子節點 index {c} 不大於父節點"
                )));
            }
            if rec(c, 5) != i as i32 {
                return Err(WzjsError::Format(format!(
                    "node {i} 宣告子節點 {c}，但其 parent={}",
                    rec(c, 5)
                )));
            }
        }
    }

    // ── 值 sections 預載 ──
    // slot1: u8、slot3: u16、slot4: i32、slot5: i64、slot6: 2×f32、slot7: 2×f32、slot8: f64
    let read_u16 = |idx: i32| -> Result<u16> {
        if idx < 0 || idx as usize >= sec_count(3) {
            return Err(WzjsError::IndexOutOfRange(format!(
                "slot3 index {idx} 越界（count {}）",
                sec_count(3)
            )));
        }
        let relative = (idx as usize)
            .checked_mul(2)
            .ok_or_else(|| WzjsError::Format("slot3 index 溢位".into()))?;
        let pos = sec_off(3)
            .checked_add(relative)
            .ok_or_else(|| WzjsError::Format("slot3 offset 溢位".into()))?;
        let b = data.get(pos..pos + 2).ok_or(WzjsError::Truncated {
            pos,
            need: 2,
            len: data.len(),
        })?;
        Ok(u16::from_le_bytes(b.try_into().unwrap()))
    };
    let read_u8 = |idx: i32| -> Result<u8> {
        if idx < 0 || idx as usize >= sec_count(1) {
            return Err(WzjsError::IndexOutOfRange(format!(
                "slot1 index {idx} 越界（count {}）",
                sec_count(1)
            )));
        }
        let pos = sec_off(1)
            .checked_add(idx as usize)
            .ok_or_else(|| WzjsError::Format("slot1 offset 溢位".into()))?;
        data.get(pos).copied().ok_or(WzjsError::Truncated {
            pos,
            need: 1,
            len: data.len(),
        })
    };

    // ── 倒序組樹（子節點 index 一定大於父節點，倒序處理時子樹已完成）──
    let mut built: Vec<Option<WzNode>> = (0..node_count).map(|_| None).collect();
    for i in (0..node_count).rev() {
        let type_tag = rec(i, 0);
        let f2 = rec(i, 2);
        let child_count = rec(i, 4) as usize;
        let path_idx = rec(i, 6);
        let ct = rec(i, 7) as usize;

        let name_idx = rec(i, 1);
        if name_idx < 0 {
            return Err(WzjsError::Format(format!(
                "node {i} 名稱 index 為負: {name_idx}"
            )));
        }
        let name = pool_a
            .get(name_idx as usize)
            .ok_or_else(|| {
                WzjsError::IndexOutOfRange(format!(
                    "node {i} 名稱 index {name_idx} 越界（pool A {} 筆）",
                    pool_a.len()
                ))
            })?
            .clone();
        let path = if path_idx < 0 {
            None
        } else {
            Some(
                pool_b
                    .get(path_idx as usize)
                    .ok_or_else(|| {
                        WzjsError::IndexOutOfRange(format!(
                            "node {i} 路徑 index {path_idx} 越界（pool B {} 筆）",
                            pool_b.len()
                        ))
                    })?
                    .clone(),
            )
        };

        let value = match type_tag {
            crate::wzjs::node::TYPE_SUBTREE => Value::SubTree,
            type_tag if crate::wzjs::node::is_canvas_tag(type_tag) => Value::Canvas,
            3 => Value::U8(read_u8(f2)?),
            5 => Value::U16(read_u16(f2)?),
            6 => Value::Int(sec_u32(4, f2)? as i32),
            7 => {
                let (lo, hi) = sec_u32x2(5, f2)?;
                Value::Long(((hi as u64) << 32 | lo as u64) as i64)
            }
            8 => {
                let (a, b) = sec_u32x2(6, f2)?;
                // 格式層無法區分 2×f32 與 2×i32（同一屬性名兩種寫法都存在）。
                // 啟發式：任一半字是 denormal（指數全 0 且非零）→ 整對視為 int bits；
                // 真實遊戲資料不會出現 denormal float。全量普查：2003 筆中僅 8 筆 int。
                let denormal = |bits: u32| bits != 0 && (bits & 0x7F80_0000) == 0;
                if denormal(a) || denormal(b) {
                    Value::Int2(a as i32, b as i32)
                } else {
                    Value::Float2(f32::from_bits(a), f32::from_bits(b))
                }
            }
            9 => {
                let (lo, hi) = sec_u32x2(8, f2)?;
                Value::Double(f64::from_bits((hi as u64) << 32 | lo as u64))
            }
            11 => Value::Str(read_pool_c(f2, i, &pool_c)?),
            12 => Value::Uol(read_pool_c(f2, i, &pool_c)?),
            14 => {
                let (x, y) = sec_u32x2(7, f2)?;
                Value::Vector(f32::from_bits(x), f32::from_bits(y))
            }
            t => return Err(WzjsError::UnknownType(t, i as u32)),
        };

        let mut children = Vec::with_capacity(child_count);
        for k in 0..child_count {
            let c = child_table[ct + k] as usize;
            children.push(
                built[c]
                    .take()
                    .ok_or_else(|| WzjsError::Format(format!("node {c} 被重複引用")))?,
            );
        }

        built[i] = Some(WzNode {
            name,
            value,
            path,
            children,
            type_tag,
        });
    }

    if rec(0, 5) != -1 {
        return Err(WzjsError::Format(
            "node 0 不是根節點（parent != -1）".into(),
        ));
    }
    if built.iter().skip(1).any(Option::is_some) {
        return Err(WzjsError::Format("存在未連到 node 0 的節點".into()));
    }
    built[0]
        .take()
        .ok_or_else(|| WzjsError::Format("根節點缺失".into()))
}
