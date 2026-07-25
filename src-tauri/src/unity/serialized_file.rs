//! SerializedFile v22 解析（header / types / objects / scriptTypes / externals）。
//!
//! v22 header 佈局（擴充欄位一律 **big-endian**）：
//! ```text
//! [0..4)   legacy metadataSize u32（通常 0）
//! [4..8)   legacy fileSize u32
//! [8..12)  version u32 = 22
//! [12..16) legacy dataOffset u32
//! [16]     endianness u8（0 = little）+ [17..20) reserved
//! [20..24) metadataSize u32   ┐
//! [24..32) fileSize i64       │ v22 新增，big-endian
//! [32..40) dataOffset i64     │
//! [40..48) unknown i64        ┘
//! ```
//! 之後的 metadata 本體改用 endianness byte 指定的位元序：
//! unityVersion cstr | targetPlatform i32 | enableTypeTree u8
//! | types | objects | scriptTypes | externals
//!
//! 這批 bundle 實測 enableTypeTree = false，所以 type 只有 classID 等基本欄位，
//! 物件欄位全靠 `objects.rs` 硬編佈局解析。

use super::{Reader, Result, UnityError};

pub const SF_VERSION: u32 = 22;
/// v22 header 固定 48 bytes。
pub const HEADER_SIZE: usize = 48;
const MAX_METADATA_SIZE: usize = 256 * 1024 * 1024;
const MAX_TABLE_ENTRIES: i32 = 1_000_000;

fn table_count(r: &mut Reader<'_>, label: &str, min_entry_size: usize) -> Result<usize> {
    let count = r.i32()?;
    if !(0..=MAX_TABLE_ENTRIES).contains(&count) {
        return Err(UnityError::Format(format!("{label} 數量異常: {count}")));
    }
    let count = count as usize;
    if min_entry_size != 0 && count > r.remaining() / min_entry_size {
        return Err(UnityError::Format(format!(
            "{label} 數量超出 metadata 剩餘空間: {count}（剩餘 {} bytes）",
            r.remaining()
        )));
    }
    Ok(count)
}

#[derive(Debug)]
pub struct SerializedType {
    pub class_id: i32,
    pub is_stripped: bool,
    pub script_type_index: i16,
}

#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub path_id: i64,
    /// 相對於 dataOffset 的起點。
    pub byte_start: i64,
    pub byte_size: u32,
    /// types 表的 index。
    pub type_index: usize,
    pub class_id: i32,
}

#[derive(Debug)]
pub struct ScriptType {
    pub local_serialized_file_index: i32,
    pub local_identifier_in_file: i64,
}

#[derive(Debug)]
pub struct External {
    pub guid: [u8; 16],
    pub type_: i32,
    pub path_name: String,
}

#[derive(Debug)]
pub struct SerializedFile {
    pub unity_version: String,
    pub target_platform: i32,
    pub little_endian: bool,
    /// 物件資料在 node 內的起點（object 絕對位置 = data_offset + byte_start）。
    pub data_offset: u64,
    pub types: Vec<SerializedType>,
    pub objects: Vec<ObjectInfo>,
    pub script_types: Vec<ScriptType>,
    pub externals: Vec<External>,
}

impl SerializedFile {
    /// 從 node 開頭的 bytes 解析。`head` 至少要涵蓋 header + 全部 metadata
    /// （呼叫端先用 [`metadata_len`] 決定要讀多少）。
    pub fn parse(head: &[u8]) -> Result<SerializedFile> {
        if head.len() < HEADER_SIZE {
            return Err(UnityError::Truncated {
                pos: 0,
                need: HEADER_SIZE,
                have: head.len(),
            });
        }
        // header 固定 big-endian
        let mut r = Reader::new(&head[..HEADER_SIZE], false);
        let _legacy_metadata_size = r.u32()?;
        let _legacy_file_size = r.u32()?;
        let version = r.u32()?;
        if version != SF_VERSION {
            return Err(UnityError::BadSerializedFile(version));
        }
        let _legacy_data_offset = r.u32()?;
        let endianness = r.u8()?;
        if endianness > 1 {
            return Err(UnityError::Format(format!(
                "endianness 值異常: {endianness}"
            )));
        }
        r.skip(3)?; // reserved
        let metadata_size = r.u32()? as usize;
        if metadata_size > MAX_METADATA_SIZE || metadata_size > head.len() - HEADER_SIZE {
            return Err(UnityError::Format(format!(
                "metadataSize 異常: {metadata_size}（可用 {} bytes）",
                head.len() - HEADER_SIZE
            )));
        }
        let file_size = r.i64()?;
        if file_size < 0 {
            return Err(UnityError::Format(format!("fileSize 為負: {file_size}")));
        }
        let data_offset = r.i64()?;
        if data_offset < 0 {
            return Err(UnityError::Format(format!(
                "dataOffset 為負: {data_offset}"
            )));
        }
        if file_size != 0 && data_offset > file_size {
            return Err(UnityError::Format(format!(
                "dataOffset {data_offset} 超過 fileSize {file_size}"
            )));
        }
        let data_offset = data_offset as u64;
        let _unknown = r.i64()?;

        let little = endianness == 0;
        let mut r = Reader::new(&head[HEADER_SIZE..HEADER_SIZE + metadata_size], little);

        let unity_version = r.cstring()?;
        let target_platform = r.i32()?;
        let enable_type_tree = r.bool()?;
        if enable_type_tree {
            // 本專案面對的 bundle 全部 enableTypeTree = false；
            // type tree 佈局（含 typeDependencies）不在支援範圍內。
            return Err(UnityError::Format(
                "enableTypeTree = true 不在支援範圍".into(),
            ));
        }

        // types（enableTypeTree = false 前提下的精簡佈局）
        let type_count = table_count(&mut r, "types", 23)?;
        let mut types = Vec::with_capacity(type_count);
        for _ in 0..type_count {
            let class_id = r.i32()?;
            let is_stripped = r.bool()?; // v16+
            let script_type_index = r.i16()?; // v17+
            if class_id == 114 {
                r.skip(16)?; // scriptID hash（MonoBehaviour）
            }
            r.skip(16)?; // oldTypeHash
                         // 注意：typeDependencies（v21+）只在 enableTypeTree = true 時才存在，
                         // 這裡沒有。
            types.push(SerializedType {
                class_id,
                is_stripped,
                script_type_index,
            });
        }

        // objects
        let object_count = table_count(&mut r, "objects", 24)?;
        let mut objects = Vec::with_capacity(object_count);
        for _ in 0..object_count {
            r.align4(); // v14+
            let path_id = r.i64()?;
            let byte_start = r.i64()?; // v22+ 是 i64
            if byte_start < 0 {
                return Err(UnityError::Format(format!(
                    "object {path_id} byteStart 為負: {byte_start}"
                )));
            }
            let byte_size = r.u32()?;
            let type_index = r.i32()?; // v16+ 是 types 表 index
            if type_index < 0 || type_index as usize >= types.len() {
                return Err(UnityError::Format(format!(
                    "object type index 越界: {type_index}"
                )));
            }
            let class_id = types[type_index as usize].class_id;
            if file_size != 0 {
                let object_end = data_offset
                    .checked_add(byte_start as u64)
                    .and_then(|start| start.checked_add(byte_size as u64))
                    .ok_or_else(|| {
                        UnityError::Format(format!("object {path_id} byte range 溢位"))
                    })?;
                if object_end > file_size as u64 {
                    return Err(UnityError::Format(format!(
                        "object {path_id} 結尾 {object_end} 超過 fileSize {file_size}"
                    )));
                }
            }
            objects.push(ObjectInfo {
                path_id,
                byte_start,
                byte_size,
                type_index: type_index as usize,
                class_id,
            });
        }

        // scriptTypes
        let script_count = table_count(&mut r, "scriptTypes", 12)?;
        let mut script_types = Vec::with_capacity(script_count);
        for _ in 0..script_count {
            let local_serialized_file_index = r.i32()?;
            r.align4();
            let local_identifier_in_file = r.i64()?;
            script_types.push(ScriptType {
                local_serialized_file_index,
                local_identifier_in_file,
            });
        }

        // externals
        let externals_count = table_count(&mut r, "externals", 22)?;
        let mut externals = Vec::with_capacity(externals_count);
        for _ in 0..externals_count {
            let _temp_empty = r.cstring()?; // v6+
            let guid: [u8; 16] = r.take(16)?.try_into().unwrap();
            let type_ = r.i32()?;
            let path_name = r.cstring()?;
            externals.push(External {
                guid,
                type_,
                path_name,
            });
        }
        // 之後可能還有 userInformation cstring，本專案用不到，不解析。

        Ok(SerializedFile {
            unity_version,
            target_platform,
            little_endian: little,
            data_offset,
            types,
            objects,
            script_types,
            externals,
        })
    }

    /// 依 classID 過濾物件。
    pub fn objects_by_class(&self, class_id: i32) -> impl Iterator<Item = &ObjectInfo> {
        self.objects.iter().filter(move |o| o.class_id == class_id)
    }
}

/// 讀 node 開頭 48 bytes，回傳 header + metadata 的總長度（呼叫端據此決定要餵給
/// [`SerializedFile::parse`] 多少 bytes）。
pub fn metadata_len(head48: &[u8]) -> Result<usize> {
    if head48.len() < HEADER_SIZE {
        return Err(UnityError::Truncated {
            pos: 0,
            need: HEADER_SIZE,
            have: head48.len(),
        });
    }
    let mut r = Reader::new(head48, false);
    r.skip(8)?;
    let version = r.u32()?;
    if version != SF_VERSION {
        return Err(UnityError::BadSerializedFile(version));
    }
    r.skip(8)?; // legacy dataOffset + endianness + reserved
    let metadata_size = r.u32()? as usize;
    if metadata_size > MAX_METADATA_SIZE {
        return Err(UnityError::Format(format!(
            "metadataSize 超過上限: {metadata_size} > {MAX_METADATA_SIZE}"
        )));
    }
    HEADER_SIZE
        .checked_add(metadata_size)
        .ok_or_else(|| UnityError::Format("metadata 長度溢位".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 組一個最小 v22 SerializedFile（LE metadata，1 type + 1 object + 1 external）。
    fn build_minimal_sf() -> Vec<u8> {
        let mut meta = Vec::new();
        meta.extend_from_slice(b"6000.3.16f1\0"); // unityVersion
        meta.extend_from_slice(&39i32.to_le_bytes()); // targetPlatform (Android)
        meta.push(0); // enableTypeTree = false
                      // types: 1 筆 TextAsset(49)
        meta.extend_from_slice(&1i32.to_le_bytes());
        meta.extend_from_slice(&49i32.to_le_bytes()); // classID
        meta.push(0); // isStripped
        meta.extend_from_slice(&(-1i16).to_le_bytes()); // scriptTypeIndex
        meta.extend_from_slice(&[0u8; 16]); // oldTypeHash
                                            // （enableTypeTree = false 時沒有 typeDependencies）
                                            // objects: 1 筆
        meta.extend_from_slice(&1i32.to_le_bytes());
        meta.extend_from_slice(&123i64.to_le_bytes()); // pathID
        meta.extend_from_slice(&0i64.to_le_bytes()); // byteStart
        meta.extend_from_slice(&64u32.to_le_bytes()); // byteSize
        meta.extend_from_slice(&0i32.to_le_bytes()); // typeIndex
                                                     // scriptTypes: 0
        meta.extend_from_slice(&0i32.to_le_bytes());
        // externals: 1 筆
        meta.extend_from_slice(&1i32.to_le_bytes());
        meta.push(0); // tempEmpty cstr
        meta.extend_from_slice(&[0xABu8; 16]); // guid
        meta.extend_from_slice(&1i32.to_le_bytes()); // type
        meta.extend_from_slice(b"resources/unity default resources\0");

        let data_offset = (HEADER_SIZE + meta.len() + 15) & !15;
        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_be_bytes()); // legacy metadataSize
        out.extend_from_slice(&0u32.to_be_bytes()); // legacy fileSize
        out.extend_from_slice(&22u32.to_be_bytes()); // version
        out.extend_from_slice(&0u32.to_be_bytes()); // legacy dataOffset
        out.push(0); // little endian
        out.extend_from_slice(&[0; 3]);
        out.extend_from_slice(&(meta.len() as u32).to_be_bytes());
        out.extend_from_slice(&0i64.to_be_bytes()); // fileSize
        out.extend_from_slice(&(data_offset as i64).to_be_bytes());
        out.extend_from_slice(&0i64.to_be_bytes()); // unknown
        out.extend_from_slice(&meta);
        out
    }

    #[test]
    fn test_parse_minimal_v22() {
        let sf_data = build_minimal_sf();
        let total = metadata_len(&sf_data[..HEADER_SIZE]).unwrap();
        assert_eq!(total, sf_data.len());
        let sf = SerializedFile::parse(&sf_data[..total]).unwrap();
        assert_eq!(sf.unity_version, "6000.3.16f1");
        assert_eq!(sf.target_platform, 39);
        assert!(sf.little_endian);
        assert_eq!(sf.types.len(), 1);
        assert_eq!(sf.types[0].class_id, 49);
        assert_eq!(sf.objects.len(), 1);
        assert_eq!(sf.objects[0].path_id, 123);
        assert_eq!(sf.objects[0].class_id, 49);
        assert_eq!(sf.externals.len(), 1);
        assert_eq!(
            sf.externals[0].path_name,
            "resources/unity default resources"
        );
    }

    #[test]
    fn test_bad_version() {
        let mut sf_data = build_minimal_sf();
        sf_data[8..12].copy_from_slice(&21u32.to_be_bytes());
        assert!(matches!(
            SerializedFile::parse(&sf_data),
            Err(UnityError::BadSerializedFile(21))
        ));
    }

    #[test]
    fn test_negative_type_count_is_rejected_without_allocation() {
        let mut sf_data = build_minimal_sf();
        let count_pos = HEADER_SIZE + b"6000.3.16f1\0".len() + 4 + 1;
        sf_data[count_pos..count_pos + 4].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(
            SerializedFile::parse(&sf_data),
            Err(UnityError::Format(message)) if message.contains("types")
        ));
    }

    #[test]
    fn test_negative_data_offset_is_rejected() {
        let mut sf_data = build_minimal_sf();
        sf_data[32..40].copy_from_slice(&(-1i64).to_be_bytes());
        assert!(matches!(
            SerializedFile::parse(&sf_data),
            Err(UnityError::Format(message)) if message.contains("dataOffset")
        ));
    }

    #[test]
    fn test_metadata_size_limit() {
        let mut head = [0u8; HEADER_SIZE];
        head[8..12].copy_from_slice(&SF_VERSION.to_be_bytes());
        head[20..24].copy_from_slice(&(MAX_METADATA_SIZE as u32 + 1).to_be_bytes());
        assert!(metadata_len(&head).is_err());
    }
}
