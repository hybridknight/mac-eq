fn main() {
    println!("cargo:rerun-if-changed=native/audio.m");
    println!("cargo:rerun-if-changed=native/platform.m");
    println!("cargo:rerun-if-changed=native/launch.h");
    println!("cargo:rerun-if-changed=assets/MenuBarTemplate.svg");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let svg = std::fs::read("assets/MenuBarTemplate.svg").expect("Missing menu-bar SVG");
        let bytes = svg.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
        std::fs::write(
            out_dir.join("maceq-menu-icon.h"),
            format!("static const unsigned char MacEQMenuBarSVG[] = {{{bytes}}};\n"),
        )
        .unwrap();
        let mut native = cc::Build::new();
        if std::env::var_os("CARGO_FEATURE_MEMORY_PROFILE").is_some() {
            native.define("ME_MEMORY_PROFILE", None);
        }
        native
            .warnings_into_errors(true)
            .include(&out_dir)
            .file("native/audio.m")
            .file("native/platform.m")
            .flag("-fobjc-arc")
            .flag("-mmacosx-version-min=14.4")
            .compile("maceq_audio");
        for framework in ["Foundation", "CoreAudio", "AppKit", "ServiceManagement"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
