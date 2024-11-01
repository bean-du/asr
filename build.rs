use std::process::Command;

fn main() {
    // 获取 git 版本信息
    let output = Command::new("git")
        .args(&["describe", "--tags", "--always", "--dirty"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();
    
    // 将版本信息传递给编译器
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    
    // 确保 ffmpeg 可用
    let ffmpeg_check = Command::new("ffmpeg")
        .arg("-version")
        .output();
    
    if ffmpeg_check.is_err() {
        println!("cargo:warning=ffmpeg not found in PATH, some features may not work");
    }
    
    // 设置链接配置
    println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
}