//! Unity 6000.3.16f1 硬編物件佈局。
//!
//! 欄位順序依據 AssetRipper/TypeTreeDumps 的 `StructsDump/release/6000.3.16f1.dump`
//! （官方 typetree dump），不是憑經驗推測的舊版佈局。注意 6000.3 的 Texture2D
//! **沒有** `m_ForcedFallbackFormat` / `m_DownscaleFallback`，且
//! `m_IsAlphaChannelOptional` 緊接在 m_Name 之後。
//!
//! 這些 parse 函式都吃「單一物件的完整 bytes」（byte_size 長度），
//! 以 little-endian 解析（這批 bundle 的 SerializedFile 實測為 LE）。

use super::{Reader, Result, UnityError};

// classID 常數
pub const CLASS_TEXTURE2D: i32 = 28;
pub const CLASS_TEXT_ASSET: i32 = 49;
pub const CLASS_MONO_BEHAVIOUR: i32 = 114;
pub const CLASS_MONO_SCRIPT: i32 = 115;
pub const CLASS_ASSET_BUNDLE: i32 = 142;
pub const CLASS_SPRITE: i32 = 213;
pub const CLASS_SPRITE_ATLAS: i32 = 687078895;

#[derive(Debug, Clone, Copy)]
pub struct PPtr {
    pub file_id: i32,
    pub path_id: i64,
}

fn read_pptr(r: &mut Reader) -> Result<PPtr> {
    Ok(PPtr {
        file_id: r.i32()?,
        path_id: r.i64()?,
    })
}

/// 物件 parser 實際只需要位元序，不應為此建造假的 `SerializedFile`。
pub trait EndianSource {
    fn little_endian(self) -> bool;
}

impl EndianSource for bool {
    fn little_endian(self) -> bool {
        self
    }
}

impl EndianSource for &super::serialized_file::SerializedFile {
    fn little_endian(self) -> bool {
        self.little_endian
    }
}

fn make_reader(data: &[u8], endian: impl EndianSource) -> Reader<'_> {
    Reader::new(data, endian.little_endian())
}

// ── TextAsset(49) ──────────────────────────────────────────────

#[derive(Debug)]
pub struct TextAsset {
    pub name: String,
    /// m_Script 原文（這批 bundle 是 UTF-8 JSON 明文）。
    pub script: Vec<u8>,
}

pub fn parse_text_asset(data: &[u8], endian: impl EndianSource) -> Result<TextAsset> {
    let mut r = make_reader(data, endian);
    let name = r.aligned_string()?;
    let len = r.u32()? as usize;
    if len > r.remaining() {
        return Err(UnityError::Format(format!(
            "TextAsset {name:?} m_Script 長度異常: {len}"
        )));
    }
    let script = r.take(len)?.to_vec();
    Ok(TextAsset { name, script })
}

// ── MonoScript(115) ────────────────────────────────────────────

#[derive(Debug)]
pub struct MonoScript {
    pub name: String,
    pub class_name: String,
    pub namespace: String,
    pub assembly_name: String,
}

pub fn parse_mono_script(data: &[u8], endian: impl EndianSource) -> Result<MonoScript> {
    let mut r = make_reader(data, endian);
    let name = r.aligned_string()?;
    let _execution_order = r.i32()?;
    r.skip(16)?; // m_PropertiesHash
    let class_name = r.aligned_string()?;
    let namespace = r.aligned_string()?;
    let assembly_name = r.aligned_string()?;
    Ok(MonoScript {
        name,
        class_name,
        namespace,
        assembly_name,
    })
}

// ── AssetBundle(142) ───────────────────────────────────────────

#[derive(Debug)]
pub struct ContainerEntry {
    /// 資產邏輯路徑（例如 `assets/.../blank.json`）。
    pub path: String,
    pub preload_index: i32,
    pub preload_size: i32,
    pub asset: PPtr,
}

#[derive(Debug)]
pub struct AssetBundleInfo {
    pub name: String,
    pub container: Vec<ContainerEntry>,
}

/// 只解析到 m_Container 為止（後面 m_MainAsset / m_SceneHashes 等用不到）。
pub fn parse_asset_bundle(data: &[u8], endian: impl EndianSource) -> Result<AssetBundleInfo> {
    let mut r = make_reader(data, endian);
    let name = r.aligned_string()?;
    // m_PreloadTable: vector<PPtr>
    let preload_count = r.i32()?;
    if !(0..=1_000_000).contains(&preload_count) || preload_count as usize > r.remaining() / 12 {
        return Err(UnityError::Format(format!(
            "m_PreloadTable 數量異常: {preload_count}"
        )));
    }
    for _ in 0..preload_count {
        read_pptr(&mut r)?;
    }
    // m_Container: map<string, AssetInfo>
    let container_count = r.i32()?;
    if !(0..=1_000_000).contains(&container_count) || container_count as usize > r.remaining() / 24
    {
        return Err(UnityError::Format(format!(
            "m_Container 數量異常: {container_count}"
        )));
    }
    let mut container = Vec::with_capacity(container_count as usize);
    for _ in 0..container_count {
        let path = r.aligned_string()?;
        let preload_index = r.i32()?;
        let preload_size = r.i32()?;
        let asset = read_pptr(&mut r)?;
        container.push(ContainerEntry {
            path,
            preload_index,
            preload_size,
            asset,
        });
    }
    Ok(AssetBundleInfo { name, container })
}

// ── Texture2D(28) ──────────────────────────────────────────────

#[derive(Debug)]
pub struct Texture2DInfo {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub complete_image_size: u32,
    pub mips_stripped: i32,
    pub texture_format: i32,
    pub mip_count: i32,
    pub texture_dimension: i32,
    /// 內嵌 image data 的大小（streamed 時為 0）。
    pub image_data_size: u32,
    /// 內嵌 image data 在 Texture2D 物件內的精確起點。
    pub image_data_offset: usize,
    /// m_StreamData：像素在 .resS node 的 (offset, size, path)。
    pub stream_offset: u64,
    pub stream_size: u32,
    pub stream_path: String,
}

/// 依 6000.3.16f1 typetree dump 的欄位順序。
pub fn parse_texture2d(data: &[u8], endian: impl EndianSource) -> Result<Texture2DInfo> {
    let mut r = make_reader(data, endian);
    let name = r.aligned_string()?;
    let _is_alpha_channel_optional = r.bool()?; // MetaFlag align
    r.align4();
    let width = r.i32()?;
    let height = r.i32()?;
    if width <= 0 || height <= 0 || width > 32_768 || height > 32_768 {
        return Err(UnityError::Format(format!(
            "Texture2D {name:?} 尺寸異常: {width}x{height}"
        )));
    }
    let complete_image_size = r.u32()?;
    let mips_stripped = r.i32()?;
    let texture_format = r.i32()?;
    let mip_count = r.i32()?;
    let _is_readable = r.bool()?;
    let _is_pre_processed = r.bool()?;
    let _ignore_mipmap_limit = r.bool()?; // MetaFlag align
    r.align4();
    let _mipmap_limit_group_name = r.aligned_string()?;
    let _streaming_mipmaps = r.bool()?; // MetaFlag align
    r.align4();
    let _streaming_mipmaps_priority = r.i32()?;
    let _image_count = r.i32()?;
    let texture_dimension = r.i32()?;
    // GLTextureSettings
    let _filter_mode = r.i32()?;
    let _aniso = r.i32()?;
    let _mip_bias = r.f32()?;
    let _wrap_u = r.i32()?;
    let _wrap_v = r.i32()?;
    let _wrap_w = r.i32()?;
    let _lightmap_format = r.i32()?;
    let _color_space = r.i32()?;
    let _platform_blob = r.byte_vec()?;
    // image data（TypelessData：u32 size + raw bytes，不讀內容只跳過）
    let image_data_size = r.u32()?;
    let image_data_offset = r.pos();
    if image_data_size as usize > r.remaining() {
        return Err(UnityError::Format(format!(
            "Texture2D {name:?} image data 長度異常: {image_data_size}（剩餘 {}）",
            r.remaining()
        )));
    }
    r.skip(image_data_size as usize)?;
    r.align4(); // MetaFlag 0x4001：align after
                // m_StreamData
    let stream_offset = r.u64()?;
    let stream_size = r.u32()?;
    let stream_path = r.aligned_string()?;

    Ok(Texture2DInfo {
        name,
        width,
        height,
        complete_image_size,
        mips_stripped,
        texture_format,
        mip_count,
        texture_dimension,
        image_data_size,
        image_data_offset,
        stream_offset,
        stream_size,
        stream_path,
    })
}

// ── Sprite(213) ────────────────────────────────────────────────
// Phase 1 只需要 m_Name 與 m_Rect；Phase 4 需要 m_RD.texture（圖片來源）。

#[derive(Debug)]
pub struct SpriteInfo {
    pub name: String,
    /// m_Rect: (x, y, width, height)
    pub rect: (f32, f32, f32, f32),
    /// m_RD.texture（像素所在的 Texture2D）。
    pub texture: PPtr,
}

pub fn parse_sprite(data: &[u8], endian: impl EndianSource) -> Result<SpriteInfo> {
    let mut r = make_reader(data, endian);
    let name = r.aligned_string()?;
    let rect = (r.f32()?, r.f32()?, r.f32()?, r.f32()?);
    // m_Offset Vector2f
    r.skip(8)?;
    // m_Border Vector4f
    r.skip(16)?;
    // m_PixelsToUnits f32 + m_Pivot Vector2f + m_Extrude u32
    r.skip(4 + 8 + 4)?;
    // m_IsPolygon bool + align
    r.u8()?;
    r.align4();
    // m_RenderDataKey: GUID(16) + SInt64(8)
    r.skip(24)?;
    // m_AtlasTags: vector<string>
    let tag_count = r.i32()?;
    if !(0..=4096).contains(&tag_count) {
        return Err(UnityError::Format(format!(
            "Sprite {name:?} m_AtlasTags 數量異常: {tag_count}"
        )));
    }
    for _ in 0..tag_count {
        r.aligned_string()?;
    }
    // m_SpriteAtlas PPtr
    read_pptr(&mut r)?;
    // m_RD.texture PPtr（m_RD 第一個欄位）
    let texture = read_pptr(&mut r)?;
    Ok(SpriteInfo {
        name,
        rect,
        texture,
    })
}

// ── MonoBehaviour(114) ─────────────────────────────────────────
// 只解析共用 header 欄位；m_Name 之後的全部 bytes 是自訂 payload
// （大 json bundle 的 WZJS v5 素材），原樣保留給 Phase 2。

#[derive(Debug)]
pub struct MonoBehaviourInfo {
    pub game_object: PPtr,
    pub enabled: bool,
    pub script: PPtr,
    pub name: String,
    /// header 佔用的 bytes 數；payload = `data[header_len..]`。
    pub header_len: usize,
}

pub fn parse_mono_behaviour(data: &[u8], endian: impl EndianSource) -> Result<MonoBehaviourInfo> {
    let mut r = make_reader(data, endian);
    let game_object = read_pptr(&mut r)?;
    let enabled = r.bool()?;
    r.align4();
    let script = read_pptr(&mut r)?;
    let name = r.aligned_string()?;
    Ok(MonoBehaviourInfo {
        game_object,
        enabled,
        script,
        name,
        header_len: r.pos(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unity::serialized_file::HEADER_SIZE;

    fn put_aligned_string(buf: &mut Vec<u8>, s: &str) {
        buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
        while !buf.len().is_multiple_of(4) {
            buf.push(0);
        }
    }

    #[test]
    fn test_parse_text_asset() {
        let mut buf = Vec::new();
        put_aligned_string(&mut buf, "ItemTable");
        let json = br#"{"a":1}"#;
        buf.extend_from_slice(&(json.len() as u32).to_le_bytes());
        buf.extend_from_slice(json);
        let ta = parse_text_asset(&buf, true).unwrap();
        assert_eq!(ta.name, "ItemTable");
        assert_eq!(ta.script, json);
    }

    #[test]
    fn test_parse_mono_behaviour_header() {
        // 模擬 blank：12 (PPtr) + 4 (enabled+align) + 12 (PPtr) + string + payload
        let mut buf = Vec::new();
        buf.extend_from_slice(&0i32.to_le_bytes()); // m_GameObject.fileID
        buf.extend_from_slice(&111i64.to_le_bytes()); // m_GameObject.pathID
        buf.push(1); // m_Enabled
        buf.extend_from_slice(&[0; 3]); // align
        buf.extend_from_slice(&0i32.to_le_bytes()); // m_Script.fileID
        buf.extend_from_slice(&222i64.to_le_bytes()); // m_Script.pathID
        put_aligned_string(&mut buf, "blank");
        let header_len = buf.len();
        buf.extend_from_slice(b"WZJS\x05\x00\x00\x00"); // payload 開頭

        let mb = parse_mono_behaviour(&buf, true).unwrap();
        assert_eq!(mb.game_object.path_id, 111);
        assert!(mb.enabled);
        assert_eq!(mb.script.path_id, 222);
        assert_eq!(mb.name, "blank");
        assert_eq!(mb.header_len, header_len);
        assert_eq!(&buf[mb.header_len..][..4], b"WZJS");
    }

    #[test]
    fn test_parse_texture2d_layout() {
        // 依 6000.3.16f1 typetree 組一個合成 Texture2D，驗證欄位對齊
        let mut buf = Vec::new();
        put_aligned_string(&mut buf, "tex_test");
        buf.push(0); // m_IsAlphaChannelOptional
        buf.extend_from_slice(&[0; 3]); // align
        buf.extend_from_slice(&512i32.to_le_bytes()); // width
        buf.extend_from_slice(&256i32.to_le_bytes()); // height
        buf.extend_from_slice(&524288u32.to_le_bytes()); // completeImageSize
        buf.extend_from_slice(&0i32.to_le_bytes()); // mipsStripped
        buf.extend_from_slice(&4i32.to_le_bytes()); // format = RGBA32
        buf.extend_from_slice(&1i32.to_le_bytes()); // mipCount
        buf.extend_from_slice(&[1, 0, 0]); // isReadable, isPreProcessed, ignoreMipmapLimit
        buf.push(0); // align padding
        put_aligned_string(&mut buf, ""); // mipmapLimitGroupName
        buf.push(0); // streamingMipmaps
        buf.extend_from_slice(&[0; 3]); // align
        buf.extend_from_slice(&0i32.to_le_bytes()); // streamingMipmapsPriority
        buf.extend_from_slice(&1i32.to_le_bytes()); // imageCount
        buf.extend_from_slice(&2i32.to_le_bytes()); // textureDimension
                                                    // GLTextureSettings
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&0f32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&6i32.to_le_bytes()); // lightmapFormat
        buf.extend_from_slice(&1i32.to_le_bytes()); // colorSpace
        buf.extend_from_slice(&0u32.to_le_bytes()); // platformBlob = empty
        buf.extend_from_slice(&0u32.to_le_bytes()); // image data size = 0
                                                    // m_StreamData
        buf.extend_from_slice(&4096u64.to_le_bytes()); // offset
        buf.extend_from_slice(&524288u32.to_le_bytes()); // size
        put_aligned_string(&mut buf, "archive:/cab.resS");

        let tex = parse_texture2d(&buf, true).unwrap();
        assert_eq!(tex.name, "tex_test");
        assert_eq!(tex.width, 512);
        assert_eq!(tex.height, 256);
        assert_eq!(tex.texture_format, 4);
        assert_eq!(tex.mip_count, 1);
        assert_eq!(tex.stream_offset, 4096);
        assert_eq!(tex.stream_size, 524288);
        assert_eq!(tex.stream_path, "archive:/cab.resS");
        assert_eq!(tex.image_data_offset, 104);
        assert_eq!(HEADER_SIZE, 48); // 防止手滑改到 v22 header 大小
    }

    #[test]
    fn test_mono_behaviour_truncated_name_is_error() {
        let mut buf = vec![0; 28];
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_mono_behaviour(&buf, true).is_err());
    }

    #[test]
    fn test_texture_rejects_negative_dimensions() {
        let mut buf = Vec::new();
        put_aligned_string(&mut buf, "bad");
        buf.extend_from_slice(&[0; 4]);
        buf.extend_from_slice(&(-1i32).to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        assert!(parse_texture2d(&buf, true).is_err());
    }
}
