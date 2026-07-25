//! Unity 容器層：UnityFS bundle、SerializedFile、硬編物件佈局。

pub mod bundle;
pub mod objects;
pub mod serialized_file;
pub mod texture;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum UnityError {
    #[error("I/O 錯誤: {0}")]
    Io(#[from] std::io::Error),

    #[error("不是 UnityFS 檔案（signature = {0:?}）")]
    BadSignature(String),

    #[error("不支援的 UnityFS 版本: {0}")]
    UnsupportedBundleVersion(u32),

    #[error("不支援的 blocksInfo 壓縮方式: {0}")]
    UnsupportedBlocksInfoCompression(u32),

    #[error("不支援的 block 壓縮方式: {0}（僅支援 raw / LZ4 / LZ4HC）")]
    UnsupportedBlockCompression(u16),

    #[error("LZ4 解壓失敗: {0}")]
    Lz4(String),

    #[error("不是 SerializedFile（version = {0}）")]
    BadSerializedFile(u32),

    #[error("資料截斷：需要 {need} bytes，剩下 {have} bytes（at offset {pos:#x}）")]
    Truncated {
        pos: usize,
        need: usize,
        have: usize,
    },

    #[error("格式錯誤: {0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, UnityError>;

/// 向上對齊到 `align` 的倍數。
pub fn align_up(v: u64, align: u64) -> u64 {
    v.div_ceil(align) * align
}

/// 可失敗的向上對齊。解析不可信輸入時應使用這個版本，避免加法溢位。
pub fn checked_align_up(v: u64, align: u64) -> Option<u64> {
    if align == 0 || !align.is_power_of_two() {
        return None;
    }
    v.checked_add(align - 1).map(|n| n & !(align - 1))
}

/// 可切換位元序的游標式讀取器（SerializedFile metadata 用）。
///
/// Unity 的對齊規則：string / byte vector 讀完後 align(4)；
/// 帶 `align` MetaFlag 的 bool 欄位讀完後 align(4)。
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    /// true = little-endian（bundle 內的 SerializedFile 實測為 LE）
    little: bool,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8], little_endian: bool) -> Self {
        Reader {
            buf,
            pos: 0,
            little: little_endian,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn is_little_endian(&self) -> bool {
        self.little
    }

    pub fn set_little_endian(&mut self, little: bool) {
        self.little = little;
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// 對齊到 4 bytes 邊界（相對於 buffer 起點）。
    pub fn align4(&mut self) {
        self.pos = (self.pos + 3) & !3;
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(UnityError::Truncated {
                pos: self.pos,
                need: n,
                have: self.remaining(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(if self.little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(if self.little {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(if self.little {
            u64::from_le_bytes(b.try_into().unwrap())
        } else {
            u64::from_be_bytes(b.try_into().unwrap())
        })
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(self.u64()? as i64)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// 讀 null 結尾字串。
    pub fn cstring(&mut self) -> Result<String> {
        let start = self.pos;
        while self.pos < self.buf.len() && self.buf[self.pos] != 0 {
            self.pos += 1;
        }
        if self.pos >= self.buf.len() {
            return Err(UnityError::Truncated {
                pos: start,
                need: 1,
                have: 0,
            });
        }
        let s = String::from_utf8_lossy(&self.buf[start..self.pos]).into_owned();
        self.pos += 1; // 跳過 null
        Ok(s)
    }

    /// Unity aligned string：u32 長度 + bytes + align(4)。
    pub fn aligned_string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        if len > self.remaining() {
            return Err(UnityError::Format(format!(
                "字串長度不合理: {len}（剩餘 {}）",
                self.remaining()
            )));
        }
        let b = self.take(len)?;
        let s = String::from_utf8_lossy(b).into_owned();
        self.align4();
        Ok(s)
    }

    /// Unity byte vector：u32 長度 + bytes + align(4)。
    pub fn byte_vec(&mut self) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > self.remaining() {
            return Err(UnityError::Format(format!(
                "byte vector 長度不合理: {len}（剩餘 {}）",
                self.remaining()
            )));
        }
        let b = self.take(len)?.to_vec();
        self.align4();
        Ok(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, 16), 0);
        assert_eq!(align_up(1, 16), 16);
        assert_eq!(align_up(16, 16), 16);
        assert_eq!(align_up(17, 16), 32);
        assert_eq!(align_up(0x243, 16), 0x250);
        assert_eq!(checked_align_up(17, 16), Some(32));
        assert_eq!(checked_align_up(u64::MAX, 16), None);
        assert_eq!(checked_align_up(1, 0), None);
    }

    #[test]
    fn test_reader_endianness() {
        let buf = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let mut r = Reader::new(&buf, true);
        assert_eq!(r.u32().unwrap(), 1);
        let mut r = Reader::new(&buf, false);
        r.skip(4).unwrap();
        assert_eq!(r.u32().unwrap(), 1);
    }

    #[test]
    fn test_aligned_string() {
        // len=5 "blank" + 3 bytes padding
        let mut buf = vec![5, 0, 0, 0];
        buf.extend_from_slice(b"blank");
        buf.extend_from_slice(&[0, 0, 0]);
        buf.extend_from_slice(&[0xAA; 4]); // 下一個欄位
        let mut r = Reader::new(&buf, true);
        assert_eq!(r.aligned_string().unwrap(), "blank");
        assert_eq!(r.pos(), 12);
        assert_eq!(r.u32().unwrap(), 0xAAAAAAAA);
    }

    #[test]
    fn test_truncated_error() {
        let buf = [1u8, 2];
        let mut r = Reader::new(&buf, true);
        assert!(matches!(r.u32(), Err(UnityError::Truncated { .. })));
    }
}
