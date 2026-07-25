//! WZJS v5 解碼器 — 本案核心。
//!
//! Nexon 楓之谷經典版的自訂容器格式，包在 MonoBehaviour payload 裡。
//! 格式細節見 `format.rs` 的模組文件。

pub mod fixtures;
pub mod format;
pub mod node;

pub use format::{parse, WzjsError};
pub use node::{Value, WzNode};
