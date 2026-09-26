//! On Windows, puts the Brixo icon and name into the program itself: the
//! icon shows on the file, its shortcuts and the taskbar, and Task Manager
//! says "Brixo Player". On macOS it builds the link catcher instead.
fn main() {
    println!("cargo:rerun-if-changed=../brixo-client/assets/icon.ico");
    println!("cargo:rerun-if-changed=src/mac_links.m");
    // On macOS, the little Objective-C file that catches brixo:// links.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new().file("src/mac_links.m").flag("-fobjc-arc").compile("brixo_mac_links");
        println!("cargo:rustc-link-lib=framework=Foundation");
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../brixo-client/assets/icon.ico")
        .set("ProductName", "Brixo Player")
        .set("FileDescription", "Brixo Player")
        .set("CompanyName", "Brixo")
        .set("LegalCopyright", "Brixo");
    // Needs Windows' resource compiler (part of the Build Tools). Without it
    // the program still builds, just with the plain icon.
    if let Err(e) = res.compile() {
        println!("cargo:warning=no Brixo icon in the program ({e})");
    }
}
