//! WZSS 索引解析與 atlas 裁切。

use crate::unity::texture;
use crate::unity::{Result, UnityError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WzssRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 一個 frame 在 atlas 上的完整位置：哪一頁 + 該頁內的 rect。
///
/// rect 座標是「單頁內」的（實測 x/y+w/h 皆 < 頁寬高），頁號另外存在
/// frame 記錄裡，兩者缺一不可。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WzssSprite {
    pub page: usize,
    pub rect: WzssRect,
}

/// atlas 的一頁（已解碼 RGBA，仍是 Unity 的 bottom-up 排列）。
pub struct AtlasPage {
    /// 檔名 `_N` 後綴的頁號，即 frame 記錄裡的頁索引。
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// WZSS 中明確保存的 UOL → sprite frame 關係。
///
/// Canvas 的定位主線是 [`WzssIndex::frame_index_for_node`] 的節點鍵；這條
/// UOL 反推只在極少數節點沒有對應鍵時當備援（全量 703,698 個 canvas 中 156 個）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WzssUolLink {
    pub uol: String,
    /// `None` 是 WZSS 以 `frame_count` 表示的空圖片 sentinel。
    pub frame_index: Option<usize>,
}

/// frame 記錄裡真正要用的兩個欄位。
#[derive(Debug, Clone, Copy)]
struct FrameEntry {
    /// atlas 頁索引（第 3 個 i32）。
    page: usize,
    /// sprite rect 索引（第 4 個 i32）。
    rect: usize,
}

#[derive(Debug)]
pub struct WzssIndex {
    pub path: String,
    pub rects: Vec<WzssRect>,
    pub uol_links: Vec<WzssUolLink>,
    /// 節點鍵池，以鍵字串升冪排序。前 `frames.len()` 個是 item 槽
    /// （槽 i ↔ frame i），其後是別名槽（UOL 或重複圖），由 `alias_frames` 指回 frame。
    node_keys: Vec<String>,
    /// frame → (atlas 頁, sprite rect index)。
    frames: Vec<FrameEntry>,
    /// 別名槽 → frame index（`None` 是空圖 sentinel）。
    alias_frames: AliasFrames,
    /// WZSS 內部頁號 → atlas 檔名 `_N` 的頁號。兩者是不同的編號，且順序常常
    /// 不同（例：`Skill/232` 是 [3,4,2,1,0]），必須查表換算。
    page_map: Vec<usize>,
}

/// 別名槽 → frame index（`None` 是空圖 sentinel）。
type AliasFrames = Vec<(usize, Option<usize>)>;

/// 節點鍵 = `Guid(MD5(UTF8("<img 路徑>/<節點相對路徑>"))).ToString("N")`。
///
/// 逆向定案（2026-07-25）：WZSS 用這把鍵當 sprite 索引，池內以鍵字串排序供二分搜尋。
/// 位元組序是 .NET `Guid(byte[16])` 的慣例——前 4 bytes、次 2 bytes、再 2 bytes
/// 各自反轉（little-endian 欄位），其餘 8 bytes 原序。
fn node_key(full_path: &str) -> String {
    const GUID_ORDER: [usize; 16] = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
    let digest = md5::compute(full_path.as_bytes());
    let mut key = String::with_capacity(32);
    for index in GUID_ORDER {
        key.push_str(&format!("{:02x}", digest[index]));
    }
    key
}

impl WzssIndex {
    /// 由 img 內相對路徑精確定位 frame。查無鍵時回 `None`。
    pub fn frame_index_for_node(&self, inner: &str) -> Option<usize> {
        let slot = self.slot_of(&node_key(&format!("{}/{inner}", self.path)))?;
        if slot < self.frames.len() {
            Some(slot)
        } else {
            self.alias_frames
                .iter()
                .find(|(alias, _)| *alias == slot)
                .and_then(|(_, frame)| *frame)
        }
    }

    /// frame → atlas 頁（已換算成檔名 `_N` 的頁號）+ 頁內 rect。
    pub fn sprite_for_frame(&self, frame: usize) -> Option<WzssSprite> {
        let entry = self.frames.get(frame)?;
        Some(WzssSprite {
            page: *self.page_map.get(entry.page)?,
            rect: *self.rects.get(entry.rect)?,
        })
    }

    /// 池是排序過的，正常走二分搜尋；資料若不符排序不變量則退回線性搜尋。
    fn slot_of(&self, key: &str) -> Option<usize> {
        match self
            .node_keys
            .binary_search_by(|probe| probe.as_str().cmp(key))
        {
            Ok(index) => Some(index),
            Err(_) => self.node_keys.iter().position(|probe| probe == key),
        }
    }
}

/// 解析 WZSS 索引 MonoBehaviour payload（已去 Unity 標頭）。
pub fn parse_wzss(payload: &[u8]) -> Result<WzssIndex> {
    if payload.len() < 80 {
        return Err(UnityError::Format("WZSS payload 太短".into()));
    }
    let header = |index: usize| -> u32 {
        u32::from_le_bytes(
            payload[index * 4..index * 4 + 4]
                .try_into()
                .expect("80-byte header was validated"),
        )
    };
    // magic 前面是 path 與每頁 44 bytes 的 GUID 清單，頁數多時可以推到 1KB 以外
    // （`Mob/8830000` 有 28 頁、magic 在 1332）。掃描窗依頁數放寬，仍留上界避免
    // 在資料區誤命中。
    let scan = 128usize
        .saturating_add(header(19) as usize)
        .saturating_add((header(0) as usize).saturating_mul(64))
        .min(payload.len());
    let magic = payload[..scan]
        .windows(4)
        .position(|window| window == b"WZSS")
        .ok_or_else(|| UnityError::Format("WZSS magic 不存在".into()))?;
    let count = header(7) as usize;
    let offset = header(8) as usize;
    let rect_bytes = count
        .checked_mul(16)
        .ok_or_else(|| UnityError::Format("WZSS rect 大小溢位".into()))?;
    let rect_start = magic
        .checked_add(offset)
        .ok_or_else(|| UnityError::Format("WZSS rect offset 溢位".into()))?;
    let rect_end = rect_start
        .checked_add(rect_bytes)
        .ok_or_else(|| UnityError::Format("WZSS rect range 溢位".into()))?;
    if count > 1_000_000 || rect_end > payload.len() {
        return Err(UnityError::Format(format!(
            "WZSS rect 區越界: count={count} off={offset} len={}",
            payload.len()
        )));
    }

    let mut rects = Vec::with_capacity(count);
    for index in 0..count {
        let position = rect_start + index * 16;
        rects.push(WzssRect {
            x: u32::from_le_bytes(payload[position..position + 4].try_into().unwrap()),
            y: u32::from_le_bytes(payload[position + 4..position + 8].try_into().unwrap()),
            w: u32::from_le_bytes(payload[position + 8..position + 12].try_into().unwrap()),
            h: u32::from_le_bytes(payload[position + 12..position + 16].try_into().unwrap()),
        });
    }

    let page_map = parse_page_map(payload, magic, &header)?;
    let frames = parse_frames(payload, magic, &header, count, page_map.len())?;
    let (uol_links, alias_frames) = parse_uol_links(payload, magic, &header, frames.len())?;
    let node_keys = read_string_pool(
        payload,
        magic,
        header(13) as usize,
        header(14) as usize,
        header(15) as usize,
    )?;

    let path_len = header(19) as usize;
    let path = 80usize
        .checked_add(path_len)
        .filter(|end| *end <= payload.len())
        .map_or_else(String::new, |end| {
            String::from_utf8_lossy(&payload[80..end]).into_owned()
        });
    Ok(WzssIndex {
        path,
        rects,
        uol_links,
        node_keys,
        frames,
        alias_frames,
        page_map,
    })
}

/// 回傳 (UOL 記錄, 全部別名槽 → frame index)。
///
/// link 記錄是「別名」：UOL 節點（有 UOL 字串）與內容重複的 Canvas（字串索引 -1）
/// 都用它指回既有 frame，因此節點鍵池裡 item 槽之後的槽一律要走這張表。
fn parse_uol_links(
    payload: &[u8],
    magic: usize,
    header: &impl Fn(usize) -> u32,
    frame_count: usize,
) -> Result<(Vec<WzssUolLink>, AliasFrames)> {
    let string_count = header(2) as usize;
    let strings = read_string_pool(
        payload,
        magic,
        string_count,
        header(3) as usize,
        header(4) as usize,
    )?;

    let record_count = header(11) as usize;
    if record_count > 1_000_000 {
        return Err(UnityError::Format(format!(
            "WZSS link 數量異常: {record_count}"
        )));
    }
    let record_start = magic
        .checked_add(header(12) as usize)
        .ok_or_else(|| UnityError::Format("WZSS link offset 溢位".into()))?;
    let record_bytes = record_count
        .checked_mul(16)
        .ok_or_else(|| UnityError::Format("WZSS link 大小溢位".into()))?;
    let record_end = record_start
        .checked_add(record_bytes)
        .ok_or_else(|| UnityError::Format("WZSS link range 溢位".into()))?;
    if record_end > payload.len() {
        return Err(UnityError::Format(format!(
            "WZSS link 區越界: count={record_count} len={}",
            payload.len()
        )));
    }

    let mut links = Vec::new();
    let mut aliases = Vec::with_capacity(record_count);
    for index in 0..record_count {
        let position = record_start + index * 16;
        let slot = i32::from_le_bytes(payload[position..position + 4].try_into().unwrap());
        let string_index =
            i32::from_le_bytes(payload[position + 4..position + 8].try_into().unwrap());
        let frame_index =
            i32::from_le_bytes(payload[position + 12..position + 16].try_into().unwrap());
        // frame_count 本身是「空圖片」sentinel，超過才算越界。
        if frame_index < 0 || frame_index as usize > frame_count {
            return Err(UnityError::Format(format!(
                "WZSS link frame 索引越界: {frame_index}/{frame_count}"
            )));
        }
        let frame = ((frame_index as usize) < frame_count).then_some(frame_index as usize);
        if slot >= 0 {
            aliases.push((slot as usize, frame));
        }
        // -1 是「相同圖片去重」記錄，只有非負值才引用上面的 UOL 字串池。
        if string_index < 0 {
            continue;
        }
        let Some(uol) = strings.get(string_index as usize) else {
            return Err(UnityError::Format(format!(
                "WZSS link 字串索引越界: {string_index}/{}",
                strings.len()
            )));
        };
        links.push(WzssUolLink {
            uol: uol.clone(),
            frame_index: frame,
        });
    }
    Ok((links, aliases))
}

/// 讀 `WZSS` magic 之後的頁對照表：magic + 4（magic）+ 4（版本 = 4）起，
/// `header[0]` 個 u32，第 i 個是「WZSS 內部頁 i」對應的 atlas 檔名頁號。
///
/// 這張表是必要的：atlas 頁的檔名 `_N` 編號與 WZSS 內部頁號是兩套獨立編號，
/// 實測常常不同（`Skill/232` → [3,4,2,1,0]），忽略它會整頁切錯圖。
fn parse_page_map(
    payload: &[u8],
    magic: usize,
    header: &impl Fn(usize) -> u32,
) -> Result<Vec<usize>> {
    let count = header(0) as usize;
    if count == 0 || count > 4096 {
        return Err(UnityError::Format(format!("WZSS atlas 頁數異常: {count}")));
    }
    let start = magic
        .checked_add(8)
        .ok_or_else(|| UnityError::Format("WZSS 頁對照表 offset 溢位".into()))?;
    let end = count
        .checked_mul(4)
        .and_then(|bytes| start.checked_add(bytes))
        .ok_or_else(|| UnityError::Format("WZSS 頁對照表大小溢位".into()))?;
    if end > payload.len() {
        return Err(UnityError::Format(format!(
            "WZSS 頁對照表越界: count={count} len={}",
            payload.len()
        )));
    }
    let mut map = Vec::with_capacity(count);
    for index in 0..count {
        let position = start + index * 4;
        let page = u32::from_le_bytes(payload[position..position + 4].try_into().unwrap()) as usize;
        if page >= count {
            return Err(UnityError::Format(format!(
                "WZSS 頁對照表項越界: {page}/{count}"
            )));
        }
        map.push(page);
    }
    Ok(map)
}

fn parse_frames(
    payload: &[u8],
    magic: usize,
    header: &impl Fn(usize) -> u32,
    rect_count: usize,
    page_count: usize,
) -> Result<Vec<FrameEntry>> {
    let frame_count = header(9) as usize;
    if frame_count > 1_000_000 {
        return Err(UnityError::Format(format!(
            "WZSS frame 數量異常: {frame_count}"
        )));
    }
    let frame_start = magic
        .checked_add(header(10) as usize)
        .ok_or_else(|| UnityError::Format("WZSS frame offset 溢位".into()))?;
    let frame_bytes = frame_count
        .checked_mul(20)
        .ok_or_else(|| UnityError::Format("WZSS frame 大小溢位".into()))?;
    let frame_end = frame_start
        .checked_add(frame_bytes)
        .ok_or_else(|| UnityError::Format("WZSS frame range 溢位".into()))?;
    if frame_end > payload.len() {
        return Err(UnityError::Format(format!(
            "WZSS frame 區越界: count={frame_count} len={}",
            payload.len()
        )));
    }

    let mut frames = Vec::with_capacity(frame_count);
    for index in 0..frame_count {
        let position = frame_start + index * 20;
        // WzSpriteFrame 的第 3 個 i32 是 atlas 頁索引、第 4 個是 sprite rect
        // 索引。rect 座標只在單一頁內有意義，兩者要一起用。frame 數可大於
        // rect 數（不同 frame 可共用同一張去重圖片），不能直接拿 frame
        // 編號去切 atlas。
        let page = i32::from_le_bytes(payload[position + 8..position + 12].try_into().unwrap());
        let rect_index =
            i32::from_le_bytes(payload[position + 12..position + 16].try_into().unwrap());
        if rect_index < 0 || rect_index as usize >= rect_count {
            return Err(UnityError::Format(format!(
                "WZSS frame 的 rect 索引越界: {rect_index}/{rect_count}"
            )));
        }
        if page < 0 || page as usize >= page_count {
            return Err(UnityError::Format(format!(
                "WZSS frame 的 atlas 頁索引越界: {page}/{page_count}"
            )));
        }
        frames.push(FrameEntry {
            page: page as usize,
            rect: rect_index as usize,
        });
    }
    Ok(frames)
}

fn read_string_pool(
    payload: &[u8],
    magic: usize,
    count: usize,
    blob_offset: usize,
    table_offset: usize,
) -> Result<Vec<String>> {
    if count > 1_000_000 {
        return Err(UnityError::Format(format!("WZSS 字串數量異常: {count}")));
    }
    let table_start = magic
        .checked_add(table_offset)
        .ok_or_else(|| UnityError::Format("WZSS 字串表 offset 溢位".into()))?;
    let table_bytes = count
        .checked_add(1)
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| UnityError::Format("WZSS 字串表大小溢位".into()))?;
    let table_end = table_start
        .checked_add(table_bytes)
        .ok_or_else(|| UnityError::Format("WZSS 字串表 range 溢位".into()))?;
    if table_end > payload.len() {
        return Err(UnityError::Format("WZSS 字串表越界".into()));
    }

    let blob_start = magic
        .checked_add(blob_offset)
        .ok_or_else(|| UnityError::Format("WZSS 字串 blob offset 溢位".into()))?;
    let mut offsets = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let position = table_start + index * 4;
        offsets
            .push(u32::from_le_bytes(payload[position..position + 4].try_into().unwrap()) as usize);
    }

    let mut strings = Vec::with_capacity(count);
    for range in offsets.windows(2) {
        if range[0] > range[1] {
            return Err(UnityError::Format("WZSS 字串表 offset 不是遞增順序".into()));
        }
        let start = blob_start
            .checked_add(range[0])
            .ok_or_else(|| UnityError::Format("WZSS 字串起點溢位".into()))?;
        let end = blob_start
            .checked_add(range[1])
            .ok_or_else(|| UnityError::Format("WZSS 字串終點溢位".into()))?;
        let bytes = payload
            .get(start..end)
            .ok_or_else(|| UnityError::Format("WZSS 字串 blob 越界".into()))?;
        strings.push(
            std::str::from_utf8(bytes)
                .map_err(|_| UnityError::Format("WZSS 字串不是合法 UTF-8".into()))?
                .to_owned(),
        );
    }
    Ok(strings)
}

/// 從 sprite 指定的 atlas 頁（bottom-up RGBA）依 rect 切出 sprite 並翻正。
pub fn cut_sprite(atlas: &[AtlasPage], sprite: WzssSprite) -> Result<(u32, u32, Vec<u8>)> {
    let page = atlas
        .iter()
        .find(|page| page.index as usize == sprite.page)
        .ok_or_else(|| {
            UnityError::Format(format!(
                "atlas 缺少第 {} 頁（共 {} 頁）",
                sprite.page,
                atlas.len()
            ))
        })?;
    let rect = sprite.rect;
    let mut cropped = texture::crop_rgba(
        page.width,
        page.height,
        &page.rgba,
        rect.x,
        rect.y,
        rect.w,
        rect.h,
    )?;
    texture::flip_vertical(&mut cropped, rect.w, rect.h)?;
    Ok((rect.w, rect.h, cropped))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真實資料驗證向量：`Character/Cap/01000003` 的 4 個 canvas 鍵全部對得上，
    /// 且與 UOL 反推出的 frame 一致（見 docs/PLAN.md 第三輪逆向）。
    #[test]
    fn node_key_matches_real_wzss_entries() {
        assert_eq!(
            node_key("Character/Cap/01000003/default/default"),
            "3f7d61deb760a522ca04d126947f7d8c"
        );
        assert_eq!(
            node_key("Character/Cap/01000003/backDefault/default"),
            "ce8ec156f71b360968d9a13d24ef4b54"
        );
        assert_eq!(
            node_key("Character/Cap/01000003/info/icon"),
            "f06d4fe6a0dec63ece472f7a235a937c"
        );
        assert_eq!(
            node_key("Character/Cap/01000003/info/iconRaw"),
            "8b098363712917d1d31cae12e2caaf30"
        );
    }

    /// 走完整 `parse_wzss`：節點鍵池 → item 槽 / 別名槽 → frame → rect。
    #[test]
    fn node_key_lookup_resolves_item_and_alias_slots() {
        const MAGIC: usize = 128;
        const PATH: &str = "Test/Img";
        let mut keyed: Vec<(String, &str)> = ["one", "two", "three"]
            .into_iter()
            .map(|name| (node_key(&format!("{PATH}/{name}")), name))
            .collect();
        keyed.sort();

        let mut payload = vec![0u8; MAGIC + 232];
        let set_header = |payload: &mut Vec<u8>, index: usize, value: u32| {
            payload[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        };
        set_header(&mut payload, 0, 3); // atlas 頁數
        set_header(&mut payload, 7, 2); // rect count
        set_header(&mut payload, 8, 32); // rect data
        set_header(&mut payload, 9, 2); // frame count
        set_header(&mut payload, 10, 64); // frame data
        set_header(&mut payload, 11, 1); // link count
        set_header(&mut payload, 12, 104); // link data
        set_header(&mut payload, 13, 3); // 節點鍵數
        set_header(&mut payload, 14, 136); // 鍵 blob
        set_header(&mut payload, 15, 120); // 鍵 offset 表
        set_header(&mut payload, 19, PATH.len() as u32);
        payload[80..80 + PATH.len()].copy_from_slice(PATH.as_bytes());
        payload[MAGIC..MAGIC + 4].copy_from_slice(b"WZSS");
        payload[MAGIC + 4..MAGIC + 8].copy_from_slice(&4u32.to_le_bytes()); // 版本
                                                                            // 頁對照表：WZSS 頁 0/1/2 → 檔名頁 1/2/0。
        for (wzss_page, file_page) in [1u32, 2, 0].into_iter().enumerate() {
            let position = MAGIC + 8 + wzss_page * 4;
            payload[position..position + 4].copy_from_slice(&file_page.to_le_bytes());
        }

        for (index, rect) in [[0u32, 0, 10, 10], [16, 0, 20, 20]].into_iter().enumerate() {
            for (field, value) in rect.into_iter().enumerate() {
                let position = MAGIC + 32 + index * 16 + field * 4;
                payload[position..position + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        // frame 0 → WZSS 頁 2 的 rect 1、frame 1 → WZSS 頁 0 的 rect 0（刻意非恆等）。
        for (frame, (page, rect)) in [(2i32, 1i32), (0, 0)].into_iter().enumerate() {
            let position = MAGIC + 64 + frame * 20 + 8;
            payload[position..position + 4].copy_from_slice(&page.to_le_bytes());
            payload[position + 4..position + 8].copy_from_slice(&rect.to_le_bytes());
        }
        // 別名槽 2（字串索引 -1 = 重複圖）→ frame 0。
        payload[MAGIC + 104..MAGIC + 108].copy_from_slice(&2i32.to_le_bytes());
        payload[MAGIC + 108..MAGIC + 112].copy_from_slice(&(-1i32).to_le_bytes());
        payload[MAGIC + 116..MAGIC + 120].copy_from_slice(&0i32.to_le_bytes());
        for index in 0..=3 {
            let position = MAGIC + 120 + index * 4;
            payload[position..position + 4].copy_from_slice(&((index as u32) * 32).to_le_bytes());
        }
        for (index, (key, _)) in keyed.iter().enumerate() {
            let position = MAGIC + 136 + index * 32;
            payload[position..position + 32].copy_from_slice(key.as_bytes());
        }

        let parsed = parse_wzss(&payload).unwrap();
        assert_eq!(parsed.path, PATH);
        // 槽 0/1 是 item（槽 i ↔ frame i），槽 2 走別名表回到 frame 0。
        assert_eq!(parsed.frame_index_for_node(keyed[0].1), Some(0));
        assert_eq!(parsed.frame_index_for_node(keyed[1].1), Some(1));
        assert_eq!(parsed.frame_index_for_node(keyed[2].1), Some(0));
        assert_eq!(parsed.frame_index_for_node("not/in/pool"), None);
        // frame → (atlas 頁, 頁內 rect)：頁號要經過對照表換算成檔名頁號，
        // 否則會切到別頁。WZSS 頁 2 → 檔名頁 0、WZSS 頁 0 → 檔名頁 1。
        assert_eq!(
            parsed.sprite_for_frame(0),
            Some(WzssSprite {
                page: 0,
                rect: WzssRect {
                    x: 16,
                    y: 0,
                    w: 20,
                    h: 20
                },
            })
        );
        assert_eq!(
            parsed.sprite_for_frame(1),
            Some(WzssSprite {
                page: 1,
                rect: WzssRect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10
                },
            })
        );
    }

    /// 頁數多時 magic 會被 GUID 清單推到 1KB 以外（實測 `Mob/8830000` 28 頁、
    /// magic 在 1332），掃描窗必須跟著頁數放寬。
    #[test]
    fn magic_beyond_one_kilobyte_is_still_found() {
        const MAGIC: usize = 1400;
        let mut payload = vec![0u8; MAGIC + 160];
        let mut set_header = |index: usize, value: u32| {
            payload[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        };
        set_header(0, 28); // atlas 頁數
        set_header(7, 0); // rect count
        set_header(8, 32); // rect data
        set_header(9, 0); // frame count
        set_header(10, 32); // frame data
        set_header(19, 0); // path 長度
        payload[MAGIC..MAGIC + 4].copy_from_slice(b"WZSS");
        // 頁對照表：28 個 u32，恆等即可。
        for page in 0..28u32 {
            let position = MAGIC + 8 + page as usize * 4;
            payload[position..position + 4].copy_from_slice(&page.to_le_bytes());
        }
        let parsed = parse_wzss(&payload).expect("magic 應該要找得到");
        assert_eq!(parsed.rects.len(), 0);
    }

    /// atlas 頁號決定切哪一頁；只有位置對得上的那頁會被裁切。
    #[test]
    fn cut_sprite_uses_the_frame_page() {
        let solid = |value: u8| AtlasPage {
            index: 0,
            width: 2,
            height: 2,
            rgba: vec![value; 2 * 2 * 4],
        };
        let pages = vec![
            AtlasPage {
                index: 0,
                ..solid(0x11)
            },
            AtlasPage {
                index: 1,
                ..solid(0x22)
            },
        ];
        let sprite = WzssSprite {
            page: 1,
            rect: WzssRect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
        };
        let (width, height, rgba) = cut_sprite(&pages, sprite).unwrap();
        assert_eq!((width, height), (1, 1));
        assert_eq!(rgba, vec![0x22; 4]);

        // 缺頁時明確失敗，不會默默切到第 0 頁。
        let missing = WzssSprite { page: 5, ..sprite };
        assert!(cut_sprite(&pages, missing).is_err());
    }
}
