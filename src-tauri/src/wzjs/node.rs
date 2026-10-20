//! 解出的節點樹模型（+ serde 序列化）。

use serde::Serialize;

pub const TYPE_SUBTREE: i32 = 2;
pub const TYPE_UOL: i32 = 12;
pub const TYPE_CANVAS: i32 = 18;
pub const TYPE_CANVAS_LINKED: i32 = 19;

pub fn is_canvas_tag(type_tag: i32) -> bool {
    matches!(type_tag, TYPE_CANVAS | TYPE_CANVAS_LINKED)
}

/// Canvas 節點本身有像素；UOL 節點是連結，但 WZSS 節點鍵池同樣收錄了它，
/// 多數能直接查到別名指向的 frame（見 `image/wzss.rs`）。兩者都值得嘗試取圖。
pub fn is_image_capable_tag(type_tag: i32) -> bool {
    is_canvas_tag(type_tag) || type_tag == TYPE_UOL
}

/// 節點值。type tag 與值的對應是逆向實測定案（見 `format.rs` 文件）：
///
/// | type tag | 值 |
/// |---|---|
/// | 2 | SubTree（無值，純階層節點） |
/// | 3 | u8（slot1 section，實測為 0/1 bool 類） |
/// | 5 | u16（slot3 section） |
/// | 6 | i32（slot4 section） |
/// | 7 | i64（slot5 section） |
/// | 8 | 數值對 8 bytes（slot6 section）：通常是 2×f32（fs / mobRate / probability），
///     但同一屬性名（swim / recovery）在不同 img 裡有 int bits 與 float bits 兩種寫法——
///     格式層無法區分，以「denormal 即 int」啟發式分流成 [`Value::Int2`] |
/// | 9 | f64（slot8 section，如 unitPrice） |
/// | 11 | 字串（pool C，如 z = 圖層名、islot；對應舊版 WZ 的 String property） |
/// | 12 | uol 連結（pool C，如 `../../alert/1/pants`；對應舊版 WZ 的 Uol property） |
/// | 14 | vector 2×f32（slot7 section，origin / map 點） |
/// | 18 / 19 | Canvas（圖片節點，像素在 spriteset/spritesheet，靠 path 關聯） |
///
/// 舊版 WZ 用字串存的列舉值（z 圖層名、islot 等），在 WZJS 收進 pool C，
/// 以 type 11 索引；真正的整數一律走 type 6（section i32）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    SubTree,
    Canvas,
    U8(u8),
    U16(u16),
    Int(i32),
    Long(i64),
    /// type 8：section entry 固定 8 bytes = 2 個 f32（語意依節點而定）。
    Float2(f32, f32),
    /// type 8 的變體：bits 為 denormal（指數全 0 且非零）時判定是 int bits。
    /// 實測全量 2003 筆 type 8 中只有 8 筆（swim×6、recovery×2）。
    Int2(i32, i32),
    Double(f64),
    Vector(f32, f32),
    /// type 11：pool C 字串（z / islot 等列舉字串）。
    Str(String),
    /// type 12：pool C uol 連結路徑。
    Uol(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct WzNode {
    pub name: String,
    pub value: Value,
    /// pool B 路徑：從 img 根出發的相對路徑（根節點則是資產路徑，
    /// 例如 `Character/Afterimage/blank`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<WzNode>,
    /// 原始 type tag（偵錯與匯出對照用，不序列化）。
    #[serde(skip)]
    pub type_tag: i32,
}

impl WzNode {
    /// 依路徑段逐一往下找子節點（名稱區分大小寫）。
    pub fn find(&self, path: &str) -> Option<&WzNode> {
        let mut cur = self;
        for seg in path.split('/') {
            cur = cur.children.iter().find(|c| c.name == seg)?;
        }
        Some(cur)
    }

    /// 節點總數（含自己）。
    pub fn count_nodes(&self) -> usize {
        1 + self.children.iter().map(WzNode::count_nodes).sum::<usize>()
    }
}
