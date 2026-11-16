//! Windows resource compilation: the fox icon and the version block.
//! `winresource` drives `windres` on the GNU toolchain (and `rc.exe`
//! equivalents elsewhere); everything is a no-op on other platforms.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = std::path::Path::new("assets/icon.ico");
    if !icon.exists() {
        println!("cargo:warning=assets/icon.ico missing; exe ships without an icon");
        return;
    }
    if let Err(error) = winresource::WindowsResource::new()
        .set_icon("assets/icon.ico")
        .set("FileDescription", "OpenFox — the Rust browser")
        .set("ProductName", "OpenFox")
        .set("LegalCopyright", "Copyright (c) 2026 Mohsen Tavana. All rights reserved.")
        .set("OriginalFilename", "openfox.exe")
        .compile()
    {
        println!("cargo:warning=winresource failed: {error}");
    }
}
