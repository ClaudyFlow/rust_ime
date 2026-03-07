fn main() {
    slint_build::compile("src/ui/main.slint").unwrap();

    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("picture/rust-ime.ico");
        res.compile().unwrap();
    }
}
