fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Icon plus the version details shown in the exe's Properties > Details tab.
        // FileVersion/ProductVersion come from the Cargo package version.
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "SetMTU")
            .set("FileDescription", "SetMTU - network interface MTU utility")
            .set("CompanyName", "riaanjutte")
            .set("LegalCopyright", "Copyright (c) 2026 riaanjutte. MIT licence.")
            .set("OriginalFilename", "SetMTU.exe")
            .set("InternalName", "SetMTU");
        res.compile().expect("failed to embed Windows resources");
    }
}
