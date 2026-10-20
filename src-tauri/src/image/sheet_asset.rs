//! 新版 SpriteSheet `.asset` 的 MonoBehaviour payload。
//!
//! 依序為 img 路徑、atlas 頁 GUID 清單、實際 sprite（路徑/頁/rect/pivot），
//! 以及別名（路徑 → sprite 序號）。字串採 Unity 的 4-byte 對齊格式。

use std::collections::HashMap;

use crate::unity::{Result, UnityError};

use super::{WzssRect, WzssSprite};

#[derive(Debug)]
pub struct SheetAsset {
    pub path: String,
    sprites: Vec<WzssSprite>,
    paths: HashMap<String, Option<usize>>,
}

impl SheetAsset {
    pub fn sprite_for_node(&self, inner: &str) -> Option<WzssSprite> {
        self.paths
            .get(inner)
            .and_then(|index| index.and_then(|index| self.sprites.get(index).copied()))
    }

    pub fn first_sprite(&self) -> Option<WzssSprite> {
        self.sprites.first().copied()
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| UnityError::Format("SpriteSheet asset 長度溢位".into()))?;
        let bytes = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| UnityError::Format("SpriteSheet asset 資料越界".into()))?;
        self.pos = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn pixel(&mut self, allow_zero: bool) -> Result<u32> {
        let value = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        let rounded = value.round();
        if !value.is_finite()
            || value < 0.0
            || (!allow_zero && value == 0.0)
            || (value - rounded).abs() > 0.001
            || rounded > u32::MAX as f32
        {
            return Err(UnityError::Format(format!(
                "SpriteSheet asset rect 不是有效整數像素: {value}"
            )));
        }
        Ok(rounded as u32)
    }

    fn string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        if len > 1_000_000 {
            return Err(UnityError::Format("SpriteSheet asset 字串過長".into()));
        }
        let value = std::str::from_utf8(self.take(len)?)
            .map_err(|_| UnityError::Format("SpriteSheet asset 字串不是 UTF-8".into()))?
            .to_owned();
        let padding = (4 - len % 4) % 4;
        self.take(padding)?;
        Ok(value)
    }

    fn count(&mut self) -> Result<usize> {
        let count = self.u32()? as usize;
        if count > 1_000_000 {
            return Err(UnityError::Format("SpriteSheet asset 項目過多".into()));
        }
        Ok(count)
    }
}

pub fn parse_sheet_asset(payload: &[u8]) -> Result<SheetAsset> {
    let mut cursor = Cursor {
        bytes: payload,
        pos: 0,
    };
    let path = cursor.string()?;
    let page_count = cursor.count()?;
    for _ in 0..page_count {
        cursor.string()?; // atlas GUID
        cursor.take(8)?; // 頁的保留欄位
    }

    let sprite_count = cursor.count()?;
    let mut sprites = Vec::with_capacity(sprite_count);
    let mut paths = HashMap::with_capacity(sprite_count);
    for index in 0..sprite_count {
        let name = cursor.string()?;
        let page = cursor.i32()?;
        if page < 0 || page as usize >= page_count {
            return Err(UnityError::Format(format!(
                "SpriteSheet asset 頁號越界: {page}/{page_count}"
            )));
        }
        let rect = WzssRect {
            x: cursor.pixel(true)?,
            y: cursor.pixel(true)?,
            w: cursor.pixel(false)?,
            h: cursor.pixel(false)?,
        };
        cursor.take(8)?; // pivot (2 × f32)，裁圖不需使用
        sprites.push(WzssSprite {
            page: page as usize,
            rect,
        });
        paths.insert(name, Some(index));
    }

    let alias_count = cursor.count()?;
    for _ in 0..alias_count {
        let name = cursor.string()?;
        let index = cursor.i32()?;
        // sprite_count 是空圖片 sentinel，對應 WZSS 的既有慣例。
        if index < 0 || index as usize > sprite_count {
            return Err(UnityError::Format(format!(
                "SpriteSheet asset 別名序號越界: {index}/{sprite_count}"
            )));
        }
        paths.insert(
            name,
            (index as usize != sprite_count).then_some(index as usize),
        );
    }

    Ok(SheetAsset {
        path,
        sprites,
        paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
        out.resize(out.len() + (4 - value.len() % 4) % 4, 0);
    }

    #[test]
    fn parses_sprite_alias_and_empty_sentinel() {
        let mut payload = Vec::new();
        string(&mut payload, "Character/00002000");
        payload.extend_from_slice(&1u32.to_le_bytes());
        string(&mut payload, "0123456789abcdef0123456789abcdef");
        payload.extend_from_slice(&[0; 8]);
        payload.extend_from_slice(&1u32.to_le_bytes());
        string(&mut payload, "stand1/0/body");
        payload.extend_from_slice(&0i32.to_le_bytes());
        for value in [10.0f32, 20.0, 30.0, 40.0, 0.5, 0.5] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        payload.extend_from_slice(&2u32.to_le_bytes());
        string(&mut payload, "walk1/0/body");
        payload.extend_from_slice(&0i32.to_le_bytes());
        string(&mut payload, "empty");
        payload.extend_from_slice(&1i32.to_le_bytes());

        let asset = parse_sheet_asset(&payload).unwrap();
        assert_eq!(asset.path, "Character/00002000");
        let expected = WzssSprite {
            page: 0,
            rect: WzssRect {
                x: 10,
                y: 20,
                w: 30,
                h: 40,
            },
        };
        assert_eq!(asset.sprite_for_node("stand1/0/body"), Some(expected));
        assert_eq!(asset.sprite_for_node("walk1/0/body"), Some(expected));
        assert_eq!(asset.sprite_for_node("empty"), None);
        assert_eq!(asset.sprite_for_node("missing"), None);
    }
}
