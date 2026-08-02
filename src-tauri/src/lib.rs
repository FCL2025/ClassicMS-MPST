//! 楓之谷經典版資料查看工具 — Phase 1：Unity 容器層
//!
//! 解析 Nexon 楓之谷經典版（Unity 6000.3.16f1）的 asset bundle：
//! UnityFS（LZ4 區塊隨機存取）→ SerializedFile v22 → 硬編物件佈局。

pub mod audio;
pub mod avatar;
pub mod catalog;
pub mod commodity;
pub mod export;
#[cfg(feature = "desktop")]
pub mod gui;
pub mod image;
pub mod item_tooltip;
pub mod monster;
pub mod search;
pub mod unity;
#[cfg(target_arch = "wasm32")]
pub mod wasm_shims;
pub mod wzjs;
