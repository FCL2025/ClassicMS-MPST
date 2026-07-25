//! 圖片查詢與渲染；索引建構留在父模組，WZSS 格式由 `wzss` 子模組處理。

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{split_img_prefix, CatalogError, DataService, EntryLocation, Result};
use crate::unity::{objects, texture, UnityError};

use super::{cut_sprite, parse_wzss, SetSpriteEntry, SheetEntry, WzssUolLink};

impl DataService {
    /// 讀取 EntryLocation 指向的物件 bytes。
    pub fn read_entry(&self, loc: &EntryLocation) -> Result<Vec<u8>> {
        self.validate_location(loc)?;
        let bundle = self
            .bundles
            .get(loc.bundle_index as usize)
            .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
        Ok(bundle.read_node_range(
            loc.node_index as usize,
            loc.abs_offset,
            loc.byte_size as usize,
        )?)
    }

    /// 取 canvas 或 uol 節點的圖片（PNG bytes）。查無圖時回 None。
    pub fn get_image(&self, path: &str) -> Result<Option<Vec<u8>>> {
        if let Some(image) = self.cached_image(path)? {
            return Ok(Some(image.as_ref().clone()));
        }
        let image = self.get_image_uncached(path)?;
        if let Some(bytes) = image {
            let cached = self.cache_image(path, bytes)?;
            Ok(Some(cached.as_ref().clone()))
        } else {
            Ok(None)
        }
    }

    fn get_image_uncached(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let (img_key, inner) = match split_img_prefix(&self.catalog, path) {
            Some(parts) => parts,
            None => return Ok(None),
        };
        // 圖片只屬於 Canvas 與 UOL（UOL 在 WZSS 節點鍵池裡也有自己的槽，多數能
        // 直接解出別名指向的 frame）。過去 frame 對不上時會回整張 atlas，讓任意
        // 深層純資料節點看起來像有圖片；在查索引前先以 WZJS 樹明確限縮。
        if !inner.is_empty() {
            let tree = self.tree_for(&img_key)?;
            let Some(node) = tree.find(&inner) else {
                return Ok(None);
            };
            if !crate::wzjs::node::is_image_capable_tag(node.type_tag) {
                return Ok(None);
            }
        }

        let mut candidates = Vec::new();
        if !inner.is_empty() {
            candidates.push(format!("{img_key}/{inner}"));
            if let Some((parent, _)) = img_key.rsplit_once('/') {
                candidates.push(format!("{parent}/{inner}"));
            }
        }
        candidates.push(img_key.clone());
        for candidate in &candidates {
            if let Some(entry) = self.catalog.image.sheets.get(candidate).cloned() {
                let per_img = *candidate == img_key;
                return self
                    .render_sheet(&entry, &img_key, &inner, per_img)
                    .map(Some);
            }
        }

        if let Some(asset) = self.catalog.image.set_assets.get(&img_key).cloned() {
            if let Some(entry) = pick_set_sprite(&asset, &inner) {
                return self.render_set_sprite(entry).map(Some);
            }
        }
        Ok(None)
    }

    /// 精確 frame 無法由 WZSS 節點鍵解出時，提供完整 atlas 頁作為預覽。
    ///
    /// 僅接受已驗證的 Canvas 或 UOL 內部節點；不讓 img 根或一般 SubTree 誤顯示 atlas。
    pub fn get_canvas_atlas_fallback(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let (img_key, inner) = match split_img_prefix(&self.catalog, path) {
            Some(parts) if !parts.1.is_empty() => parts,
            _ => return Ok(None),
        };
        let tree = self.tree_for(&img_key)?;
        let Some(node) = tree.find(&inner) else {
            return Ok(None);
        };
        if !crate::wzjs::node::is_image_capable_tag(node.type_tag) {
            return Ok(None);
        }

        let mut candidates = vec![format!("{img_key}/{inner}")];
        if let Some((parent, _)) = img_key.rsplit_once('/') {
            candidates.push(format!("{parent}/{inner}"));
        }
        candidates.push(img_key);

        for candidate in candidates {
            let Some(entry) = self.catalog.image.sheets.get(&candidate) else {
                continue;
            };
            let Some((_, location)) = entry.atlas_pages.first() else {
                continue;
            };
            return self.render_atlas_page(location).map(Some);
        }
        Ok(None)
    }

    /// spritesheet → PNG（frame 切圖；映射未知時只有 img 根可輸出整頁）。
    fn render_sheet(
        &self,
        entry: &SheetEntry,
        img_key: &str,
        inner: &str,
        per_img: bool,
    ) -> Result<Vec<u8>> {
        let index_loc = entry.index_loc.as_ref().ok_or_else(|| {
            CatalogError::Unity(UnityError::Format(
                "spritesheet 缺少 .wzspritesheet 索引".into(),
            ))
        })?;
        let payload = self.read_entry(index_loc)?;
        let mono = objects::parse_mono_behaviour(&payload, true)?;
        let wzss_payload = payload
            .get(mono.header_len..)
            .ok_or_else(|| UnityError::Format("WZSS MonoBehaviour header 越界".into()))?;
        let wzss = parse_wzss(wzss_payload)?;

        // 先用 WZSS 的節點鍵精確定位；查不到才退回 UOL 反推。
        let mut frame = if inner.is_empty() {
            None
        } else {
            wzss.rect_index_for_node(inner)
        };
        if frame.is_none() {
            if per_img && !inner.is_empty() {
                let tree = self.tree_for(img_key)?;
                frame = infer_frame_from_uol(&tree, inner, &wzss.uol_links);
            } else if !wzss.rects.is_empty() {
                frame = Some(0);
            }
        }

        let mut pages = Vec::new();
        if frame.is_some() || !entry.atlas_pages.is_empty() {
            for (_, loc) in &entry.atlas_pages {
                let data = self.read_entry(loc)?;
                let texture_info = parse_texture2d_le(&data)?;
                let bundle = self
                    .bundles
                    .get(loc.bundle_index as usize)
                    .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
                let rgba =
                    texture::fetch_pixels(bundle, loc.node_index as usize, &texture_info, &data)?;
                pages.push((texture_info.width as u32, texture_info.height as u32, rgba));
            }
        }

        if let (Some(frame_index), true) = (frame, !pages.is_empty()) {
            if let Some(rect) = wzss.rects.get(frame_index).copied() {
                let (width, height, rgba) = cut_sprite(&pages, rect)?;
                return texture::to_png(width, height, &rgba).map_err(CatalogError::Unity);
            }
        }
        if inner.is_empty() {
            let Some((width, height, mut rgba)) = pages.into_iter().next() else {
                return Err(UnityError::Format("spritesheet 無 atlas 頁".into()).into());
            };
            texture::flip_vertical(&mut rgba, width, height)?;
            return Ok(texture::to_png(width, height, &rgba)?);
        }
        Err(UnityError::Format(format!(
            "找不到 Canvas {img_key}/{inner} 可驗證的 spritesheet frame（rect {}）",
            wzss.rects.len(),
        ))
        .into())
    }

    fn render_atlas_page(&self, location: &EntryLocation) -> Result<Vec<u8>> {
        let data = self.read_entry(location)?;
        let texture_info = parse_texture2d_le(&data)?;
        let bundle = self
            .bundles
            .get(location.bundle_index as usize)
            .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
        let mut rgba =
            texture::fetch_pixels(bundle, location.node_index as usize, &texture_info, &data)?;
        let width = texture_info.width as u32;
        let height = texture_info.height as u32;
        texture::flip_vertical(&mut rgba, width, height)?;
        texture::to_png(width, height, &rgba).map_err(CatalogError::Unity)
    }

    /// spriteset sprite → PNG（m_Rect 切圖）。
    fn render_set_sprite(&self, entry: &SetSpriteEntry) -> Result<Vec<u8>> {
        let sprite_data = self.read_entry(&entry.sprite_loc)?;
        let sprite = objects::parse_sprite(&sprite_data, true)?;
        let loc = &entry.texture_loc;
        let data = self.read_entry(loc)?;
        let texture_info = parse_texture2d_le(&data)?;
        let bundle = self
            .bundles
            .get(loc.bundle_index as usize)
            .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
        let (x, y, width, height) = sprite_rect_to_pixels(sprite.rect)?;
        let rgba = texture::fetch_pixels(bundle, loc.node_index as usize, &texture_info, &data)?;
        let mut cropped = texture::crop_rgba(
            texture_info.width as u32,
            texture_info.height as u32,
            &rgba,
            x,
            y,
            width,
            height,
        )?;
        texture::flip_vertical(&mut cropped, width, height)?;
        texture::to_png(width, height, &cropped).map_err(CatalogError::Unity)
    }
}

fn parse_texture2d_le(data: &[u8]) -> Result<objects::Texture2DInfo> {
    objects::parse_texture2d(data, true).map_err(CatalogError::Unity)
}

pub(super) fn sprite_rect_to_pixels(
    rect: (f32, f32, f32, f32),
) -> crate::unity::Result<(u32, u32, u32, u32)> {
    fn exact_pixel(value: f32, label: &str, allow_zero: bool) -> crate::unity::Result<u32> {
        if !value.is_finite() || value < 0.0 || (!allow_zero && value == 0.0) {
            return Err(UnityError::Format(format!(
                "Sprite rect {label} 異常: {value}"
            )));
        }
        let rounded = value.round();
        if (value - rounded).abs() > 0.001 || rounded > u32::MAX as f32 {
            return Err(UnityError::Format(format!(
                "Sprite rect {label} 不是有效整數像素: {value}"
            )));
        }
        Ok(rounded as u32)
    }
    Ok((
        exact_pixel(rect.0, "x", true)?,
        exact_pixel(rect.1, "y", true)?,
        exact_pixel(rect.2, "width", false)?,
        exact_pixel(rect.3, "height", false)?,
    ))
}

fn pick_set_sprite<'a>(
    asset: &'a BTreeMap<String, SetSpriteEntry>,
    inner: &str,
) -> Option<&'a SetSpriteEntry> {
    if inner.is_empty() {
        return asset.values().next();
    }
    if let Some(entry) = asset.get(inner) {
        return Some(entry);
    }
    if let Some((_, entry)) = asset
        .iter()
        .find(|(key, _)| key.ends_with(&format!("/{inner}")))
    {
        return Some(entry);
    }
    let last = inner.rsplit('/').next().unwrap_or_default();
    if let Some(entry) = asset.get(last) {
        return Some(entry);
    }
    asset
        .iter()
        .find(|(key, _)| key.rsplit('/').next() == Some(last))
        .map(|(_, entry)| entry)
}

/// WZSS 的直接 Canvas 鍵是遊戲自訂 hash，rect 也以該 hash 排序，不能用
/// WZJSON DFS 順序硬配。若同一條原始 UOL 在 WZSS 中一致指向單一 frame，
/// 則可從 WZJSON 解出其目標 Canvas，得到可驗證的精確映射。
fn infer_frame_from_uol(
    root: &crate::wzjs::WzNode,
    target: &str,
    links: &[WzssUolLink],
) -> Option<usize> {
    let mut frames_by_uol: BTreeMap<&str, BTreeSet<Option<usize>>> = BTreeMap::new();
    for link in links {
        frames_by_uol
            .entry(link.uol.as_str())
            .or_default()
            .insert(link.rect_index);
    }

    fn walk(
        node: &crate::wzjs::WzNode,
        path: &str,
        target: &str,
        frames_by_uol: &BTreeMap<&str, BTreeSet<Option<usize>>>,
        candidates: &mut BTreeSet<usize>,
    ) {
        let here = if path.is_empty() {
            node.name.clone()
        } else {
            format!("{path}/{}", node.name)
        };
        if let crate::wzjs::Value::Uol(uol) = &node.value {
            if resolve_uol_target(&here, uol).as_deref() == Some(target) {
                if let Some(frames) = frames_by_uol.get(uol.as_str()) {
                    if frames.len() == 1 {
                        candidates.extend(frames.iter().flatten().copied());
                    }
                }
            }
        }
        for child in &node.children {
            walk(child, &here, target, frames_by_uol, candidates);
        }
    }

    let mut candidates = BTreeSet::new();
    // WZJSON 的根本身是資產；inner 從根的 children 開始，不含根名稱。
    for child in &root.children {
        walk(child, "", target, &frames_by_uol, &mut candidates);
    }
    (candidates.len() == 1)
        .then(|| candidates.into_iter().next())
        .flatten()
}

fn resolve_uol_target(source: &str, uol: &str) -> Option<String> {
    let mut segments: Vec<&str> = source.split('/').filter(|part| !part.is_empty()).collect();
    // UOL 是相對於該 UOL property 的父節點。
    segments.pop()?;
    for part in uol.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wzjs::{Value, WzNode};

    fn node(name: &str, value: Value, type_tag: i32, children: Vec<WzNode>) -> WzNode {
        WzNode {
            name: name.into(),
            value,
            path: None,
            children,
            type_tag,
        }
    }

    #[test]
    fn resolves_uol_from_property_parent() {
        assert_eq!(
            resolve_uol_target("blink/0/default", "../../default/default").as_deref(),
            Some("default/default")
        );
        assert_eq!(
            resolve_uol_target("stunned/0/default", "../../bewildered/0/default").as_deref(),
            Some("bewildered/0/default")
        );
        assert_eq!(
            resolve_uol_target("blaze/1/default", "../0/default").as_deref(),
            Some("blaze/0/default")
        );
    }

    #[test]
    fn infers_only_a_unique_verified_uol_frame() {
        let tree = node(
            "01010000",
            Value::SubTree,
            2,
            vec![
                node(
                    "default",
                    Value::SubTree,
                    2,
                    vec![node("default", Value::Canvas, 18, vec![])],
                ),
                node(
                    "blink",
                    Value::SubTree,
                    2,
                    vec![node(
                        "0",
                        Value::SubTree,
                        2,
                        vec![node(
                            "default",
                            Value::Uol("../../default/default".into()),
                            12,
                            vec![],
                        )],
                    )],
                ),
            ],
        );
        let links = vec![
            WzssUolLink {
                uol: "../../default/default".into(),
                rect_index: Some(7),
            },
            WzssUolLink {
                uol: "../../default/default".into(),
                rect_index: Some(7),
            },
        ];
        assert_eq!(
            infer_frame_from_uol(&tree, "default/default", &links),
            Some(7)
        );

        let ambiguous = vec![
            links[0].clone(),
            WzssUolLink {
                uol: "../../default/default".into(),
                rect_index: Some(3),
            },
        ];
        assert_eq!(
            infer_frame_from_uol(&tree, "default/default", &ambiguous),
            None
        );
    }
}
