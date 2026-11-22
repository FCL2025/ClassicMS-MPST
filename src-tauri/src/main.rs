//! Tauri app 二進位入口。

// release 版隱藏背景 console 視窗；debug 版保留（方便看 log / panic 輸出）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    msclassic::gui::run();
}
