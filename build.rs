fn main() {
    slint_build::compile("src/ui/main.slint").expect("Slint compilation failed");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        // 1. 使用 embed-resource 强力链接 EXE
        embed_resource::compile("icon.rc");

        // 2. 使用 winres 强力链接 DLL
        let mut res = winres::WindowsResource::new();
        res.set_icon("picture/rust-ime_v2.ico");
        res.set("FileDescription", "Rust IME (Input Method Editor)");
        res.set("ProductName", "Rust IME");
        res.compile().expect("Failed to compile Windows resources");
    }
}
