//! 單元測試 fixtures：從真實遊戲資料抽出的 WZJS payload 樣本（include_bytes 寫死，
//! 不依賴遊戲安裝路徑）。
//!
//! - `blank.bin`（768 bytes）：最小的樣本，內容完全已知（PLAN.md Phase 0 定案）
//! - `00020116.bin`（16 KB）：Character/Face，含 vector / inline int / section int / canvas
//! - `01062105.bin`（34 KB）：Character/Pants，含 link（uol）/ canvas / icon

/// 最小樣本：根 `blank` + 9 個武器類型子節點。
pub const BLANK: &[u8] = include_bytes!("fixtures/blank.bin");
/// Character/Face/00020116（260 節點）。
pub const IMG_00020116: &[u8] = include_bytes!("fixtures/00020116.bin");
/// Character/Pants/01062105（530 節點）。
pub const IMG_01062105: &[u8] = include_bytes!("fixtures/01062105.bin");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wzjs::{parse, Value};

    #[test]
    fn blank_exact_structure() {
        let root = parse(BLANK).expect("blank 解析失敗");
        assert_eq!(root.name, "blank");
        assert_eq!(root.type_tag, 2);
        assert_eq!(root.value, Value::SubTree);
        assert_eq!(root.path.as_deref(), Some("Character/Afterimage/blank"));
        assert_eq!(root.count_nodes(), 10);

        let expect = [
            "axe", "crossBow", "mace", "poleArm", "spear", "swordOL", "swordOS", "swordTL",
            "swordTS",
        ];
        assert_eq!(root.children.len(), 9);
        for (child, &name) in root.children.iter().zip(expect.iter()) {
            assert_eq!(child.name, name);
            assert_eq!(child.value, Value::SubTree);
            assert_eq!(child.path.as_deref(), Some(name));
            assert!(child.children.is_empty());
        }
    }

    #[test]
    fn img_00020116_face() {
        let root = parse(IMG_00020116).expect("00020116 解析失敗");
        assert_eq!(root.name, "00020116");
        assert_eq!(root.path.as_deref(), Some("Character/Face/00020116"));
        assert_eq!(root.count_nodes(), 260);
        assert_eq!(root.children.len(), 25); // angry / bewildered / blaze / ...

        // angry/0/face：canvas，子節點 origin(vector) / map(subtree) / z(inline int)
        let face = root.find("angry/0/face").expect("缺 angry/0/face");
        assert_eq!(face.value, Value::Canvas);
        let origin = face.find("origin").expect("缺 origin");
        assert!(matches!(origin.value, Value::Vector(_, _)));
        // z 是 pool C 字串（v83 對應 STR "face"）
        let z = face.find("z").expect("缺 z");
        assert_eq!(z.value, Value::Str("face".into()));
        // blaze/0/delay：section int（第一筆 delay = 110）
        let delay = root.find("blaze/0/delay").expect("缺 blaze/0/delay");
        assert_eq!(delay.value, Value::Int(110));
    }

    #[test]
    fn img_01062105_pants() {
        let root = parse(IMG_01062105).expect("01062105 解析失敗");
        assert_eq!(root.name, "01062105");
        assert_eq!(root.path.as_deref(), Some("Character/Pants/01062105"));
        assert_eq!(root.count_nodes(), 530);

        // heal/0/pants 是 uol 連結 → ../../alert/1/pants
        let pants = root.find("heal/0/pants").expect("缺 heal/0/pants");
        assert_eq!(pants.type_tag, 12);
        assert_eq!(pants.value, Value::Uol("../../alert/1/pants".into()));

        // info/icon 是 canvas
        let icon = root.find("info/icon").expect("缺 info/icon");
        assert_eq!(icon.value, Value::Canvas);
        // info/cash 是 section int = 1
        let cash = root.find("info/cash").expect("缺 info/cash");
        assert_eq!(cash.value, Value::Int(1));
        // info/islot 是 pool C 字串（v83 對應 STR "Pn"）
        let islot = root.find("info/islot").expect("缺 info/islot");
        assert_eq!(islot.value, Value::Str("Pn".into()));
    }

    #[test]
    fn malformed_zero_nodes_is_error_not_panic() {
        let mut data = BLANK.to_vec();
        data[0..4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            parse(&data),
            Err(crate::wzjs::WzjsError::Format(message)) if message.contains("nodeCount")
        ));
    }

    #[test]
    fn malformed_huge_counts_are_bounded() {
        let mut nodes = BLANK.to_vec();
        nodes[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&nodes).is_err());

        let mut pool = BLANK.to_vec();
        pool[26 * 4..27 * 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&pool).is_err());
    }
}
