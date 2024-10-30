use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let metal_src = Path::new("/Users/douxiangbin/Documents/projects/whisper.cpp/ggml/src");
    let metal_lib = Path::new(&out_dir).join("default.metallib");

    // 编译 Metal 文件
    Command::new("xcrun")
        .args(&["-sdk", "macosx", "metal", "-c", 
                metal_src.to_str().unwrap(),
                "-o", &format!("{}/ggml-metal.air", out_dir)])
        .status()
        .unwrap();

    Command::new("xcrun")
        .args(&["-sdk", "macosx", "metallib", 
                &format!("{}/ggml-metal.air", out_dir),
                "-o", metal_lib.to_str().unwrap()])
        .status()
        .unwrap();

    println!("cargo:rustc-env=GGML_METAL_PATH_RESOURCES={}", out_dir);
    println!("cargo:rerun-if-changed={}", metal_src.to_str().unwrap());
}