//! FSB5（FMOD Sound Bank，Vorbis codec）→ Ogg Vorbis 轉封裝。
//!
//! 遊戲的 AudioClip 聲音資料是 FMOD 自訂容器：Vorbis packet 去掉 Ogg 層，
//! 每包改以 u16(LE) 長度前綴，codebook（setup header）不隨檔案攜帶，
//! 只留一個 crc32 對到 FMOD 內建的固定 setup 組。
//!
//! 重建方式（對照 Fmod5Sharp 的 FmodVorbisRebuilder，已用 ffmpeg 驗證）：
//! 1. 由 sample header 的 VORBISDATA chunk 取 crc32，查內嵌 setup 表
//!    （`vorbis_setups.bin`，13 組，來自 Fmod5Sharp 的 vorbis_headers_converted.json）。
//! 2. 自造 info / comment / setup 三個 Vorbis header packet。
//! 3. u16 前綴切出 Vorbis packet，按 block flag 累積 granule 位置，打成 Ogg page。

use std::collections::HashMap;
use std::sync::OnceLock;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("FSB5 格式錯誤: {0}")]
    Format(String),

    #[error("未知的 vorbis setup crc32: {0:#010x}（遊戲更新可能需要更新 setup 表）")]
    UnknownSetup(u32),
}

pub type Result<T> = std::result::Result<T, AudioError>;

// ── 內嵌 vorbis setup 表 ───────────────────────────────────────

/// 二進位格式：u32 筆數，之後每筆 u32 crc32 + u32 seek_bit + u32 header 長度 + header bytes。
static SETUPS_BIN: &[u8] = include_bytes!("vorbis_setups.bin");

struct SetupEntry {
    seek_bit: u32,
    header: &'static [u8],
}

fn setups() -> &'static HashMap<u32, SetupEntry> {
    static TABLE: OnceLock<HashMap<u32, SetupEntry>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut map = HashMap::new();
        let take_u32 = |data: &mut &[u8]| {
            let (v, rest) = data.split_at(4);
            *data = rest;
            u32::from_le_bytes(v.try_into().unwrap())
        };
        let mut data = SETUPS_BIN;
        let count = take_u32(&mut data);
        for _ in 0..count {
            let crc32 = take_u32(&mut data);
            let seek_bit = take_u32(&mut data);
            let len = take_u32(&mut data) as usize;
            let (header, rest) = data.split_at(len);
            data = rest;
            map.insert(crc32, SetupEntry { seek_bit, header });
        }
        map
    })
}

// ── FSB5 sample header（u64 位元欄位）──────────────────────────

/// FrequencyId → 實際取樣率（FMOD 固定表）。
const FREQUENCIES: [u32; 11] = [
    4_000, 8_000, 11_000, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 96_000,
];

const FSB5_HEADER_SIZE: usize = 60;
const CODEC_VORBIS: u32 = 0x0f;
const CHUNK_VORBISDATA: u32 = 11;

// ── LSB-first 位元讀取器（vorbis 位元序）───────────────────────

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read(&mut self, bits: usize) -> Result<u64> {
        let mut value = 0u64;
        for i in 0..bits {
            let byte = self
                .data
                .get(self.pos / 8)
                .ok_or_else(|| AudioError::Format("bit reader 越界".into()))?;
            value |= u64::from((byte >> (self.pos % 8)) & 1) << i;
            self.pos += 1;
        }
        Ok(value)
    }
}

/// 由 setup header 解析每個 mode 的 block flag（0 = 短窗 256、1 = 長窗 2048）。
/// `seek_bit` 是 modes 區塊在 setup header 內的起始 bit 位置（查表值）。
fn block_flags(setup: &[u8], seek_bit: u32) -> Result<Vec<u8>> {
    let mut r = BitReader::new(setup);
    if r.read(8)? != 5 {
        return Err(AudioError::Format("setup header 不是 packet type 5".into()));
    }
    for _ in 0..6 {
        r.read(8)?; // "vorbis"
    }
    r.pos = seek_bit as usize;
    let num_modes = r.read(6)? + 1;
    if num_modes > 64 {
        return Err(AudioError::Format(format!("mode 數量異常: {num_modes}")));
    }
    let mut flags = Vec::with_capacity(num_modes as usize);
    for _ in 0..num_modes {
        flags.push(r.read(1)? as u8);
        r.read(16)?;
        r.read(16)?;
        r.read(8)?;
    }
    Ok(flags)
}

/// 單一 vorbis packet 的 block size（非音訊 packet 回 0）。
fn packet_block_size(packet: &[u8], flags: &[u8]) -> u32 {
    if packet.is_empty() {
        return 0;
    }
    let mut r = BitReader::new(packet);
    if r.read(1).unwrap_or(1) == 1 {
        return 0;
    }
    let mode = if flags.len() > 1 {
        r.read(flags.len() - 1).unwrap_or(0) as usize
    } else {
        0
    };
    if flags.get(mode).copied().unwrap_or(0) == 1 {
        2048
    } else {
        256
    }
}

// ── Ogg page 輸出 ──────────────────────────────────────────────

fn ogg_crc_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (i, entry) in table.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 {
                    (r << 1) ^ 0x04C1_1DB7
                } else {
                    r << 1
                };
            }
            *entry = r;
        }
        table
    })
}

fn ogg_crc(data: &[u8]) -> u32 {
    let table = ogg_crc_table();
    let mut crc = 0u32;
    for &b in data {
        crc = (crc << 8) ^ table[((crc >> 24) as u8 ^ b) as usize];
    }
    crc
}

/// 組一個 Ogg page（多 packet，自動 lacing）。
fn push_page(
    out: &mut Vec<u8>,
    serial: u32,
    seq: u32,
    granule: u64,
    packets: &[&[u8]],
    bos: bool,
    eos: bool,
) {
    let mut segments = Vec::new();
    let mut body = Vec::new();
    for packet in packets {
        body.extend_from_slice(packet);
        let mut n = packet.len();
        while n >= 255 {
            segments.push(255u8);
            n -= 255;
        }
        segments.push(n as u8);
    }
    let header_type = (if bos { 2 } else { 0 }) | (if eos { 4 } else { 0 });
    let mut page = Vec::with_capacity(27 + segments.len() + body.len());
    page.extend_from_slice(b"OggS");
    page.push(0); // version
    page.push(header_type);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&serial.to_le_bytes());
    page.extend_from_slice(&seq.to_le_bytes());
    page.extend_from_slice(&0u32.to_le_bytes()); // crc 占位
    page.push(segments.len() as u8);
    page.extend_from_slice(&segments);
    page.extend_from_slice(&body);
    let crc = ogg_crc(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&page);
}

// ── 主流程 ─────────────────────────────────────────────────────

/// 把一整個 FSB5（單 sample、Vorbis codec）轉成可播放的 Ogg Vorbis。
pub fn fsb_to_ogg(fsb: &[u8]) -> Result<Vec<u8>> {
    if fsb.len() < FSB5_HEADER_SIZE || &fsb[..4] != b"FSB5" {
        return Err(AudioError::Format("不是 FSB5".into()));
    }
    let u32_at = |p: usize| -> Result<u32> {
        fsb.get(p..p + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .ok_or_else(|| AudioError::Format("FSB5 header 截斷".into()))
    };
    let num_samples = u32_at(8)?;
    let sh_size = u32_at(12)? as usize;
    let name_size = u32_at(16)? as usize;
    let data_size = u32_at(20)? as usize;
    let codec = u32_at(24)? & 0x1f;
    if num_samples != 1 {
        return Err(AudioError::Format(format!(
            "預期單一 sample，實際 {num_samples}"
        )));
    }
    if codec != CODEC_VORBIS {
        return Err(AudioError::Format(format!(
            "不支援的 FSB codec: {codec:#x}（只支援 Vorbis {CODEC_VORBIS:#x}）"
        )));
    }
    let data_offset = FSB5_HEADER_SIZE + sh_size + name_size;
    let data = fsb
        .get(data_offset..data_offset + data_size)
        .ok_or_else(|| AudioError::Format("FSB5 data 區截斷".into()))?;
    let sh = fsb
        .get(FSB5_HEADER_SIZE..FSB5_HEADER_SIZE + sh_size)
        .ok_or_else(|| AudioError::Format("FSB5 sample header 截斷".into()))?;
    if sh.len() < 8 {
        return Err(AudioError::Format("sample header 過短".into()));
    }

    // u64 位元欄位：bit0 hasChunks | bits1-4 freqId | bits5-6 channels
    // | bits7-33 dataOffset(×32) | bits34-63 sampleCount
    let meta = u64::from_le_bytes(sh[..8].try_into().unwrap());
    let has_chunks = meta & 1 == 1;
    let freq_id = ((meta >> 1) & 0xF) as usize;
    let channels = [1u8, 2, 6, 8][((meta >> 5) & 3) as usize];
    let sample_count = ((meta >> 34) & 0x3FFF_FFFF) as u32;
    let frequency = *FREQUENCIES
        .get(freq_id)
        .ok_or_else(|| AudioError::Format(format!("FrequencyId 越界: {freq_id}")))?;

    // chunk 鏈找 VORBISDATA 的 setup crc32
    let mut setup_crc32 = None;
    if has_chunks {
        let mut p = 8usize;
        loop {
            let info = sh
                .get(p..p + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                .ok_or_else(|| AudioError::Format("chunk header 截斷".into()))?;
            let more = info & 1 == 1;
            let csize = ((info >> 1) & 0xFF_FFFF) as usize;
            let ctype = (info >> 25) & 0x7F;
            if ctype == CHUNK_VORBISDATA {
                setup_crc32 = sh
                    .get(p + 4..p + 8)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()));
            }
            p += 4 + csize;
            if !more {
                break;
            }
        }
    }
    let setup_crc32 = setup_crc32
        .ok_or_else(|| AudioError::Format("sample header 缺少 VORBISDATA chunk".into()))?;
    let setup = setups()
        .get(&setup_crc32)
        .ok_or(AudioError::UnknownSetup(setup_crc32))?;
    let flags = block_flags(setup.header, setup.seek_bit)?;

    // u16 前綴切 packets（0 / 0xFFFF = 結束）
    let mut packets: Vec<&[u8]> = Vec::new();
    let mut pos = 0usize;
    while pos + 2 <= data.len() {
        let plen = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if plen == 0 || plen == 0xFFFF {
            break;
        }
        let Some(packet) = data.get(pos..pos + plen) else {
            break;
        };
        packets.push(packet);
        pos += plen;
    }
    if packets.is_empty() {
        return Err(AudioError::Format("沒有任何 vorbis packet".into()));
    }

    // 三個 header packet
    let mut info_pkt = Vec::with_capacity(30);
    info_pkt.push(1u8);
    info_pkt.extend_from_slice(b"vorbis");
    info_pkt.extend_from_slice(&0u32.to_le_bytes()); // version
    info_pkt.push(channels);
    info_pkt.extend_from_slice(&frequency.to_le_bytes());
    info_pkt.extend_from_slice(&[0u8; 12]); // bitrate max/nominal/min
    info_pkt.push(0b1011_1000); // blocksize: 短 256 / 長 2048
    info_pkt.push(1); // framing

    let vendor = b"msclassic";
    let mut comment_pkt = Vec::new();
    comment_pkt.push(3u8);
    comment_pkt.extend_from_slice(b"vorbis");
    comment_pkt.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    comment_pkt.extend_from_slice(vendor);
    comment_pkt.extend_from_slice(&0u32.to_le_bytes()); // 0 comments
    comment_pkt.push(1); // framing

    let serial = 0x4D53_4350; // 'MSCP'
    let mut out = Vec::with_capacity(data.len() + setup.header.len() + 4096);
    push_page(&mut out, serial, 0, 0, &[&info_pkt], true, false);
    push_page(
        &mut out,
        serial,
        1,
        0,
        &[&comment_pkt, setup.header],
        false,
        false,
    );

    // 音訊 pages：每頁 50 包，granule = (cur + prev) / 4 累積，封頂 sample_count
    let mut seq = 2u32;
    let mut granule = 0u64;
    let mut prev_bs = 0u32;
    let mut i = 0usize;
    let mut done = false;
    while i < packets.len() && !done {
        let mut chunk: Vec<&[u8]> = Vec::with_capacity(50);
        while chunk.len() < 50 && i < packets.len() && !done {
            let packet = packets[i];
            i += 1;
            let bs = packet_block_size(packet, &flags);
            if prev_bs != 0 {
                granule += u64::from(bs + prev_bs) / 4;
            }
            if granule > u64::from(sample_count) {
                granule = u64::from(sample_count);
            }
            prev_bs = bs;
            chunk.push(packet);
            if granule == u64::from(sample_count) {
                done = true;
            }
        }
        if !chunk.is_empty() {
            let eos = done || i >= packets.len();
            push_page(&mut out, serial, seq, granule, &chunk, false, eos);
            seq += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setups_table_parses() {
        let table = setups();
        assert_eq!(table.len(), 13);
        let entry = table.get(&0x8d00698d).expect("缺少已知 setup");
        assert_eq!(entry.seek_bit, 32064);
        assert_eq!(entry.header.len(), 4020);
        // block flags: 這組是 2 modes [0, 1]（已用 Python 對照驗證）
        assert_eq!(
            block_flags(entry.header, entry.seek_bit).unwrap(),
            vec![0, 1]
        );
    }

    #[test]
    fn rejects_non_fsb() {
        assert!(fsb_to_ogg(b"nope").is_err());
    }
}
