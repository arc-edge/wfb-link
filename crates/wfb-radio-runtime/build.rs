fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=src/macos_usbhost_shim.m");
        cc::Build::new()
            .file("src/macos_usbhost_shim.m")
            .flag("-fobjc-arc")
            .compile("wfb_macos_usbhost_shim");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=IOKit");
        println!("cargo:rustc-link-lib=framework=IOUSBHost");
    }
}
