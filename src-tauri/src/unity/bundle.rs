//! UnityFS bundle 解析 + LZ4 區塊隨機存取。
//!
//! UnityFS header（全部 big-endian）：
//! ```text
//! "UnityFS\0" | version u32 | unityVersion cstr | unityRevision cstr
//! size i64 | compressedBlocksInfoSize u32 | uncompressedBlocksInfoSize u32 | flags u32
//! ```
//! flags 重點位元：
//! - `0x3F`  blocksInfo 壓縮方式（0=raw, 1=LZMA, 2/3=LZ4/LZ4HC）
//! - `0x40`  blocksInfo 與 directory（node 表）合併在同一 blob
//! - `0x80`  blocksInfo 放在檔案尾端
//! - `0x200` BlockInfoNeedPaddingAtStart：blocksInfo 之後要 align(16) 才是資料區
//!
//! 另外 UnityFS version >= 7 時，header 固定欄位結束後也要先 align(16)，
//! blocksInfo 才開始（Phase 0 踩過的坑：漏掉會讀到一段 0x00，LZ4 直接爆
//! 「0 is not a valid match offset」）。
//!
//! blocksInfo blob（被 LZ4 壓縮，全部 big-endian）：
//! hash[16] | blockCount u32 | blocks[uncompressedSize u32, compressedSize u32, flags u16]
//! | nodeCount u32 | nodes[offset i64, size i64, flags u32, name cstr]
//!
//! 資料區 = 各 block 壓縮資料依序排列；node 的 (offset, size) 以
//! 「所有 block 解壓後串接起來的位址空間」為基準，所以用 uncompressed size
//! 做前綴和即可把任一 byte range 映射到少數幾個 block，只解那幾塊。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{checked_align_up, Reader, Result, UnityError};

/// bundle 檔案的隨機讀取抽象。
///
/// 桌面以 [`FileRangeRead`]（`Mutex<File>` + seek）實作；wasm 網頁版（Step 2）
/// 會改成呼叫 JS import 的 OPFS 同步讀，解析邏輯不需變動。
pub trait RangeRead: Send + Sync {
    /// 從檔案絕對 offset 讀滿 buf。
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()>;
    /// 檔案總長度。
    fn len(&self) -> u64;
    /// 檔案是否為空（clippy 慣例要求與 `len` 成對）。
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 桌面實作：共用一個 `File` 控制代碼，讀取前 seek。
pub struct FileRangeRead {
    file: Mutex<File>,
    len: u64,
}

impl FileRangeRead {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            file: Mutex::new(file),
            len,
        })
    }
}

impl RangeRead for FileRangeRead {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)
    }

    fn len(&self) -> u64 {
        self.len
    }
}

/// blocksInfo 在檔案尾端。
const FLAG_BLOCKS_INFO_AT_END: u32 = 0x80;
/// blocksInfo 之後資料區需 align(16)。
const FLAG_BLOCK_INFO_NEED_PADDING_AT_START: u32 = 0x200;
const MAX_BLOCKS_INFO_SIZE: usize = 256 * 1024 * 1024;
const MAX_BLOCK_COUNT: usize = 1_000_000;
const MAX_NODE_COUNT: usize = 1_000_000;
const MAX_BLOCK_SIZE: u32 = 512 * 1024 * 1024;
const MAX_READ_SIZE: usize = 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct BlockInfo {
    pub uncompressed_size: u32,
    pub compressed_size: u32,
    pub flags: u16,
}

#[derive(Debug)]
pub struct NodeInfo {
    /// 以解壓後資料區起點為基準的 offset。
    pub offset: i64,
    pub size: i64,
    pub flags: u32,
    pub name: String,
}

impl NodeInfo {
    /// node flags bit 2 (0x4)：此 node 是 SerializedFile（否則是 .resS 等資源檔）。
    pub fn is_serialized_file(&self) -> bool {
        self.flags & 4 != 0
    }
}

pub struct Bundle {
    reader: Box<dyn RangeRead>,
    blocks: Vec<BlockInfo>,
    /// 每個 block 的壓縮資料在檔案中的絕對位置（長度 = blocks.len()）。
    block_file_offsets: Vec<u64>,
    /// 每個 block 在「解壓後位址空間」的起始 offset（前綴和，長度 = blocks.len() + 1）。
    block_data_prefix: Vec<u64>,
    pub nodes: Vec<NodeInfo>,
    /// 簡單的 block 快取（小容量線性查找，夠用了）。
    cache: Mutex<Vec<(u32, Arc<Vec<u8>>)>>,
}

const BLOCK_CACHE_CAP: usize = 8;

impl Bundle {
    /// 開啟磁碟上的 bundle 檔（桌面路徑；wasm 用 [`Bundle::open_reader`] 接 JS 讀取）。
    pub fn open(path: &Path) -> Result<Bundle> {
        Self::open_reader(Box::new(FileRangeRead::open(path)?))
    }

    /// 從任意隨機讀取來源開啟 bundle。
    pub fn open_reader(reader: Box<dyn RangeRead>) -> Result<Bundle> {
        let file_len = reader.len();

        // ── UnityFS header（big-endian）──
        // header 結構固定但含兩個 cstring，長度不定；一次讀 256 bytes 進來解析。
        let head_len = usize::try_from(file_len.min(256))
            .map_err(|_| UnityError::Format("bundle header 長度無法表示".into()))?;
        let mut head = vec![0u8; head_len];
        reader.read_at(0, &mut head)?;
        let mut r = Reader::new(&head, false);
        let sig = r.take(8)?;
        if sig != b"UnityFS\0" {
            return Err(UnityError::BadSignature(
                String::from_utf8_lossy(sig).into_owned(),
            ));
        }
        let fs_version = r.u32()?;
        if fs_version != 8 {
            return Err(UnityError::UnsupportedBundleVersion(fs_version));
        }
        let unity_version = r.cstring()?;
        let _unity_revision = r.cstring()?;
        let bundle_size = r.i64()?;
        if bundle_size < 0 {
            return Err(UnityError::Format(format!(
                "bundle size 為負: {bundle_size}"
            )));
        }
        let bundle_size = bundle_size as u64;
        if bundle_size > file_len {
            return Err(UnityError::Format(format!(
                "bundle size {bundle_size} 超過實際檔案長度 {file_len}"
            )));
        }
        let compressed_bi_size = r.u32()? as u64;
        let uncompressed_bi_size = r.u32()? as usize;
        if compressed_bi_size == 0
            || compressed_bi_size > MAX_BLOCKS_INFO_SIZE as u64
            || uncompressed_bi_size == 0
            || uncompressed_bi_size > MAX_BLOCKS_INFO_SIZE
        {
            return Err(UnityError::Format(format!(
                "blocksInfo 大小異常: compressed={compressed_bi_size}, uncompressed={uncompressed_bi_size}"
            )));
        }
        let flags = r.u32()?;
        let header_end = checked_align_up(r.pos() as u64, 16)
            .ok_or_else(|| UnityError::Format("bundle header 對齊溢位".into()))?;
        if header_end > bundle_size {
            return Err(UnityError::Format("bundle header 超過 bundle size".into()));
        }
        let _ = unity_version;

        // ── blocksInfo blob ──
        let bi_file_pos = if flags & FLAG_BLOCKS_INFO_AT_END != 0 {
            bundle_size
                .checked_sub(compressed_bi_size)
                .ok_or_else(|| UnityError::Format("尾端 blocksInfo 起點下溢".into()))?
        } else {
            header_end
        };
        let bi_end = bi_file_pos
            .checked_add(compressed_bi_size)
            .ok_or_else(|| UnityError::Format("blocksInfo range 溢位".into()))?;
        if bi_file_pos < header_end || bi_end > bundle_size {
            return Err(UnityError::Format(format!(
                "blocksInfo range 越界: {bi_file_pos}..{bi_end}, bundle={bundle_size}"
            )));
        }
        let compressed_bi_len = usize::try_from(compressed_bi_size)
            .map_err(|_| UnityError::Format("compressed blocksInfo 長度無法表示".into()))?;
        let mut bi_raw = vec![0u8; compressed_bi_len];
        reader.read_at(bi_file_pos, &mut bi_raw)?;
        let bi = match flags & 0x3F {
            0 => {
                if compressed_bi_len != uncompressed_bi_size {
                    return Err(UnityError::Format(format!(
                        "raw blocksInfo 大小不一致: {compressed_bi_len} != {uncompressed_bi_size}"
                    )));
                }
                bi_raw
            }
            2 | 3 => {
                let mut out = vec![0u8; uncompressed_bi_size];
                lz4_flex::block::decompress_into(&bi_raw, &mut out)
                    .map_err(|e| UnityError::Lz4(format!("blocksInfo: {e}")))?;
                out
            }
            c => return Err(UnityError::UnsupportedBlocksInfoCompression(c)),
        };

        // ── 解析 blocksInfo（big-endian）──
        let mut r = Reader::new(&bi, false);
        r.skip(16)?; // hash
        let block_count = r.u32()? as usize;
        if block_count == 0 || block_count > MAX_BLOCK_COUNT || block_count > r.remaining() / 10 {
            return Err(UnityError::Format(format!(
                "block count 異常: {block_count}"
            )));
        }
        let mut blocks = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            let uncompressed_size = r.u32()?;
            let compressed_size = r.u32()?;
            let block_flags = r.u16()?;
            if uncompressed_size == 0
                || compressed_size == 0
                || uncompressed_size > MAX_BLOCK_SIZE
                || compressed_size > MAX_BLOCK_SIZE
            {
                return Err(UnityError::Format(format!(
                    "block 大小異常: compressed={compressed_size}, uncompressed={uncompressed_size}"
                )));
            }
            match block_flags & 0x3f {
                0 if compressed_size != uncompressed_size => {
                    return Err(UnityError::Format(format!(
                        "raw block 大小不一致: {compressed_size} != {uncompressed_size}"
                    )));
                }
                0 | 2 | 3 => {}
                c => return Err(UnityError::UnsupportedBlockCompression(c)),
            }
            blocks.push(BlockInfo {
                uncompressed_size,
                compressed_size,
                flags: block_flags,
            });
        }
        let node_count = r.u32()? as usize;
        if node_count > MAX_NODE_COUNT || node_count > r.remaining() / 21 {
            return Err(UnityError::Format(format!("node count 異常: {node_count}")));
        }
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let offset = r.i64()?;
            let size = r.i64()?;
            if offset < 0 || size < 0 {
                return Err(UnityError::Format(format!(
                    "node offset/size 為負: offset={offset}, size={size}"
                )));
            }
            let node_flags = r.u32()?;
            let name = r.cstring()?;
            nodes.push(NodeInfo {
                offset,
                size,
                flags: node_flags,
                name,
            });
        }

        // ── 資料區起點：0x200 旗標要求 align(16) ──
        let data_base = if flags & FLAG_BLOCKS_INFO_AT_END != 0 {
            header_end
        } else {
            header_end
                .checked_add(compressed_bi_size)
                .ok_or_else(|| UnityError::Format("資料區起點溢位".into()))?
        };
        let data_start = if flags & FLAG_BLOCK_INFO_NEED_PADDING_AT_START != 0 {
            checked_align_up(data_base, 16)
                .ok_or_else(|| UnityError::Format("資料區對齊溢位".into()))?
        } else {
            data_base
        };

        // ── block 位置表 ──
        let mut block_file_offsets = Vec::with_capacity(block_count);
        let mut block_data_prefix = Vec::with_capacity(block_count + 1);
        let mut fpos = data_start;
        let mut dpos = 0u64;
        block_data_prefix.push(0);
        for b in &blocks {
            block_file_offsets.push(fpos);
            fpos = fpos
                .checked_add(b.compressed_size as u64)
                .ok_or_else(|| UnityError::Format("壓縮 block 前綴和溢位".into()))?;
            dpos = dpos
                .checked_add(b.uncompressed_size as u64)
                .ok_or_else(|| UnityError::Format("解壓 block 前綴和溢位".into()))?;
            block_data_prefix.push(dpos);
        }
        let data_limit = if flags & FLAG_BLOCKS_INFO_AT_END != 0 {
            bi_file_pos
        } else {
            bundle_size
        };
        if data_start > data_limit || fpos > data_limit {
            return Err(UnityError::Format(format!(
                "block 資料區越界: {data_start}..{fpos}, limit={data_limit}"
            )));
        }
        for node in &nodes {
            let end = (node.offset as u64)
                .checked_add(node.size as u64)
                .ok_or_else(|| UnityError::Format(format!("node {:?} range 溢位", node.name)))?;
            if end > dpos {
                return Err(UnityError::Format(format!(
                    "node {:?} range {}..{} 超過解壓資料大小 {dpos}",
                    node.name, node.offset, end
                )));
            }
        }

        Ok(Bundle {
            reader,
            blocks,
            block_file_offsets,
            block_data_prefix,
            nodes,
            cache: Mutex::new(Vec::new()),
        })
    }

    /// 解壓後資料區的總大小。
    pub fn decompressed_size(&self) -> u64 {
        *self.block_data_prefix.last().unwrap_or(&0)
    }

    pub fn node(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// 讀取 node 內 [offset, offset+len) 的資料，只解壓落到的 block。
    pub fn read_node_range(&self, node_index: usize, offset: u64, len: usize) -> Result<Vec<u8>> {
        let node = self
            .nodes
            .get(node_index)
            .ok_or_else(|| UnityError::Format(format!("node index 越界: {node_index}")))?;
        if len > MAX_READ_SIZE {
            return Err(UnityError::Format(format!("單次讀取超過上限: {len}")));
        }
        let end = offset
            .checked_add(len as u64)
            .ok_or_else(|| UnityError::Format("node 讀取 range 溢位".into()))?;
        if end > node.size as u64 {
            return Err(UnityError::Format(format!(
                "node {:?} 讀取越界: offset={offset} len={len} size={}",
                node.name, node.size
            )));
        }
        let data_offset = (node.offset as u64)
            .checked_add(offset)
            .ok_or_else(|| UnityError::Format("node absolute offset 溢位".into()))?;
        self.read_data_range(data_offset, len)
    }

    /// 讀整個 node。
    pub fn read_node(&self, node_index: usize) -> Result<Vec<u8>> {
        let node = self
            .nodes
            .get(node_index)
            .ok_or_else(|| UnityError::Format(format!("node index 越界: {node_index}")))?;
        let size = usize::try_from(node.size).map_err(|_| {
            UnityError::Format(format!("node {:?} 太大，無法讀入記憶體", node.name))
        })?;
        self.read_node_range(node_index, 0, size)
    }

    /// 以解壓後位址空間為基準讀取 [data_offset, data_offset+len)。
    pub fn read_data_range(&self, data_offset: u64, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        if len > MAX_READ_SIZE {
            return Err(UnityError::Format(format!("單次讀取超過上限: {len}")));
        }
        let (first, last) = self.blocks_for_range(data_offset, len as u64)?;
        let mut out = Vec::with_capacity(len);
        let mut remaining = len as u64;
        let mut cur = data_offset;
        for bi in first..=last {
            let block = self.decompress_block(bi as u32)?;
            let inner = (cur - self.block_data_prefix[bi]) as usize;
            let available = block
                .len()
                .checked_sub(inner)
                .ok_or_else(|| UnityError::Format("block 內 offset 越界".into()))?;
            let take = remaining.min(available as u64) as usize;
            out.extend_from_slice(&block[inner..inner + take]);
            cur += take as u64;
            remaining -= take as u64;
        }
        Ok(out)
    }

    /// 計算 [offset, offset+len) 落在哪些 block（回傳起訖 index，含頭尾）。
    /// 抽出成純函式邏輯方便單元測試。
    fn blocks_for_range(&self, offset: u64, len: u64) -> Result<(usize, usize)> {
        blocks_for_range(&self.block_data_prefix, offset, len)
            .ok_or_else(|| UnityError::Format(format!("資料範圍越界: offset={offset} len={len}")))
    }

    /// 解壓第 index 個 block（帶快取）。
    fn decompress_block(&self, index: u32) -> Result<Arc<Vec<u8>>> {
        {
            let cache = self.cache.lock().unwrap();
            if let Some((_, data)) = cache.iter().find(|(i, _)| *i == index) {
                return Ok(data.clone());
            }
        }
        let info = self
            .blocks
            .get(index as usize)
            .ok_or_else(|| UnityError::Format(format!("block index 越界: {index}")))?;
        let mut raw = vec![0u8; info.compressed_size as usize];
        self.reader
            .read_at(self.block_file_offsets[index as usize], &mut raw)?;
        let data = match info.flags & 0x3F {
            0 => raw,
            2 | 3 => {
                let mut out = vec![0u8; info.uncompressed_size as usize];
                lz4_flex::block::decompress_into(&raw, &mut out)
                    .map_err(|e| UnityError::Lz4(format!("block {index}: {e}")))?;
                out
            }
            c => return Err(UnityError::UnsupportedBlockCompression(c)),
        };
        let data = Arc::new(data);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= BLOCK_CACHE_CAP {
            cache.remove(0);
        }
        cache.push((index, data.clone()));
        Ok(data)
    }
}

/// 純函式：給 block 前綴和與 byte range，回傳 (first_block, last_block)。
pub fn blocks_for_range(prefix: &[u64], offset: u64, len: u64) -> Option<(usize, usize)> {
    let end = offset.checked_add(len)?;
    if prefix.len() < 2 || len == 0 || end > *prefix.last()? {
        return None;
    }
    // partition_point：最後一個 prefix[i] <= offset 的 i
    let first = prefix.partition_point(|p| *p <= offset) - 1;
    let last_byte = end.checked_sub(1)?;
    let last = prefix.partition_point(|p| *p <= last_byte) - 1;
    Some((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unity::align_up;
    use std::io::Write;

    #[test]
    fn test_blocks_for_range() {
        // 3 個 block：100 / 200 / 300 bytes
        let prefix = [0, 100, 300, 600];
        assert_eq!(blocks_for_range(&prefix, 0, 50), Some((0, 0)));
        assert_eq!(blocks_for_range(&prefix, 50, 100), Some((0, 1))); // 跨到 block 1
        assert_eq!(blocks_for_range(&prefix, 100, 200), Some((1, 1)));
        assert_eq!(blocks_for_range(&prefix, 0, 600), Some((0, 2)));
        assert_eq!(blocks_for_range(&prefix, 599, 1), Some((2, 2)));
        assert_eq!(blocks_for_range(&prefix, 0, 601), None); // 越界
        assert_eq!(blocks_for_range(&prefix, 100, 0), None); // 空範圍
        assert_eq!(blocks_for_range(&prefix, u64::MAX, 2), None); // 加法溢位
    }

    /// 建立一個最小的合成 UnityFS bundle（raw blocksInfo + 2 個 LZ4 block），
    /// 驗證 header 解析、0x200 padding、跨 block 隨機讀取。
    #[test]
    fn test_synthetic_bundle_roundtrip() {
        let block0_data: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
        let block1_data: Vec<u8> = (0..300u32).map(|i| ((i * 7) % 253) as u8).collect();
        let block0_comp = lz4_flex::block::compress(&block0_data);
        let block1_comp = lz4_flex::block::compress(&block1_data);

        // blocksInfo（BE）：hash16 + blockCount + 2 blocks + nodeCount + 1 node
        let mut bi = Vec::new();
        bi.extend_from_slice(&[0u8; 16]);
        bi.extend_from_slice(&2u32.to_be_bytes());
        for (u, c) in [(&block0_data, &block0_comp), (&block1_data, &block1_comp)] {
            bi.extend_from_slice(&(u.len() as u32).to_be_bytes());
            bi.extend_from_slice(&(c.len() as u32).to_be_bytes());
            bi.extend_from_slice(&2u16.to_be_bytes()); // LZ4
        }
        bi.extend_from_slice(&1u32.to_be_bytes());
        bi.extend_from_slice(&0i64.to_be_bytes()); // node offset
        bi.extend_from_slice(&500i64.to_be_bytes()); // node size
        bi.extend_from_slice(&4u32.to_be_bytes()); // flags: serialized
        bi.extend_from_slice(b"CAB-test\0");
        let bi_comp = lz4_flex::block::compress(&bi);

        // header
        let mut file_bytes = Vec::new();
        file_bytes.extend_from_slice(b"UnityFS\0");
        file_bytes.extend_from_slice(&8u32.to_be_bytes());
        file_bytes.extend_from_slice(b"6000.3.16f1\0");
        file_bytes.extend_from_slice(b"deadbeef\0");
        let header_end = align_up(file_bytes.len() as u64 + 24, 16); // version>=7 align(16)
        let data_base = header_end + bi_comp.len() as u64;
        let data_start = align_up(data_base, 16);
        let total_size = data_start + block0_comp.len() as u64 + block1_comp.len() as u64;
        file_bytes.extend_from_slice(&(total_size as i64).to_be_bytes());
        file_bytes.extend_from_slice(&(bi_comp.len() as u32).to_be_bytes());
        file_bytes.extend_from_slice(&(bi.len() as u32).to_be_bytes());
        file_bytes.extend_from_slice(&(0x243u32).to_be_bytes()); // LZ4 + combined + padding
        file_bytes.resize(header_end as usize, 0); // version>=7 header padding
        file_bytes.extend_from_slice(&bi_comp);
        file_bytes.resize(data_start as usize, 0); // padding to 16
        file_bytes.extend_from_slice(&block0_comp);
        file_bytes.extend_from_slice(&block1_comp);

        // 寫到暫存檔再開
        let mut path = std::env::temp_dir();
        path.push(format!("msclassic_test_{}.bundle", std::process::id()));
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(&file_bytes).unwrap();
        }

        let bundle = Bundle::open(&path).unwrap();
        assert_eq!(bundle.nodes.len(), 1);
        assert_eq!(bundle.nodes[0].name, "CAB-test");
        assert!(bundle.nodes[0].is_serialized_file());
        assert_eq!(bundle.decompressed_size(), 500);

        // 全讀 = 兩個 block 串接
        let all = bundle.read_node(0).unwrap();
        let mut expect = block0_data.clone();
        expect.extend_from_slice(&block1_data);
        assert_eq!(all, expect);

        // 跨 block range 讀取
        let part = bundle.read_node_range(0, 150, 200).unwrap();
        assert_eq!(part, expect[150..350]);

        // 只讀第二個 block 後段
        let tail = bundle.read_node_range(0, 400, 100).unwrap();
        assert_eq!(tail, expect[400..500]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_negative_bundle_size_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"UnityFS\0");
        bytes.extend_from_slice(&8u32.to_be_bytes());
        bytes.extend_from_slice(b"6000.3.16f1\0");
        bytes.extend_from_slice(b"test\0");
        bytes.extend_from_slice(&(-1i64).to_be_bytes());
        bytes.extend_from_slice(&16u32.to_be_bytes());
        bytes.extend_from_slice(&16u32.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.resize(256, 0);

        let path = std::env::temp_dir().join(format!(
            "msclassic_negative_size_{}.bundle",
            std::process::id()
        ));
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(&bytes).unwrap();
        }
        assert!(matches!(
            Bundle::open(&path),
            Err(UnityError::Format(message)) if message.contains("為負")
        ));
        std::fs::remove_file(path).ok();
    }
}
