#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = hm_app::run() {
        eprintln!("Ravomi 启动失败：{error}");
        std::process::exit(1);
    }
}
