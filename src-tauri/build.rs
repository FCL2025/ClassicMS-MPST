fn main() {
    // wasm 核心版（--no-default-features）沒有 Tauri 殼，不需要產生 context。
    #[cfg(feature = "desktop")]
    tauri_build::build()
}
