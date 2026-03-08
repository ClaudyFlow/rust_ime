use std::env;
use std::path::PathBuf;

fn main() {
    slint_build::compile("src/ui/main.slint").unwrap();

    // 为 Wayland 协议生成 Rust 绑定
    if cfg!(target_os = "linux") {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let protocols_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("protocols");

        // 生成 virtual-keyboard-v1
        let vk_xml = protocols_dir.join("virtual-keyboard-v1.xml");
        if vk_xml.exists() {
            println!("cargo:rerun-if-changed=protocols/virtual-keyboard-v1.xml");
            // 注意：这里我们通常使用 wayland-scanner crate 的接口，但如果想保持零依赖构建，
            // 我们可以利用 wayland-client 自身的机制。
        }
    }

    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("picture/rust-ime.ico");
        res.compile().unwrap();
    }
}
