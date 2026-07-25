use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    build_process_tap_helper();
    tauri_build::build()
}

/// 编译 macOS Core Audio Process Tap helper，供实时字幕原生系统音频采集使用。
fn build_process_tap_helper() {
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let source = manifest_dir
            .parent()
            .unwrap()
            .join("scripts")
            .join("processTapCaptureDemo.swift");
        let target = env::var("TARGET").unwrap_or_else(|_| "aarch64-apple-darwin".to_string());
        let output_dir = manifest_dir.join("binaries");
        std::fs::create_dir_all(&output_dir).expect("failed to create helper binary directory");
        let output = output_dir.join(format!("typesass-process-tap-{}", target));
        println!("cargo:rerun-if-changed={}", source.display());
        let status = Command::new("xcrun")
            .args([
                "swiftc",
                "-parse-as-library",
                source.to_str().unwrap(),
                "-o",
                output.to_str().unwrap(),
                "-framework",
                "AppKit",
                "-framework",
                "AudioToolbox",
                "-framework",
                "AVFoundation",
                "-framework",
                "CoreAudio",
            ])
            .status()
            .expect("failed to invoke swiftc for process tap helper");
        assert!(status.success(), "failed to compile process tap helper");
    }
}
