fn main() {
    slint_build::compile("src/ui/main.slint").expect("Slint compilation failed");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("picture/rust-ime.ico");
        res.compile().expect("Failed to compile Windows resources");
    }
}
