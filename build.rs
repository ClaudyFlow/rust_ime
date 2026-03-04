fn main() {
    slint_build::compile("src/ui/main.slint").expect("Slint compilation failed");

    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("picture/rust-ime_v2.ico");
        // 如果你还想让它在控制面板显示详细版本信息，可以加下面这些：
        res.set("FileDescription", "Rust IME (Input Method Editor)");
        res.set("ProductName", "Rust IME");
        res.set("OriginalFilename", "rust-ime.exe");
        res.compile().expect("Failed to compile Windows resources");
    }
}
