fn main() {
    println!("cargo:rerun-if-changed=native/AppleMusic.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        use std::{path::PathBuf, process::Command};
        let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
            "aarch64" => "arm64", "x86_64" => "x86_64", other => panic!("unsupported Apple target {other}"),
        };
        let status = Command::new("xcrun").args(["swiftc", "-swift-version", "5", "-O", "-parse-as-library",
            "-emit-library", "-static", "-module-name", "RhineApple", "-target", &format!("{arch}-apple-macosx12.0"),
            "native/AppleMusic.swift", "-o"]).arg(out.join("libRhineApple.a")).status().expect("run Swift compiler");
        assert!(status.success(), "MusicKit bridge compilation failed");
        let swift = Command::new("xcrun").args(["--find", "swiftc"]).output().unwrap();
        let compiler = PathBuf::from(String::from_utf8(swift.stdout).unwrap().trim());
        let runtime = compiler.parent().unwrap().parent().unwrap().join("lib/swift/macosx");
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-search=native={}", runtime.display());
        println!("cargo:rustc-link-search=native=/usr/lib/swift");
        println!("cargo:rustc-link-lib=static=RhineApple");
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::build();
}
