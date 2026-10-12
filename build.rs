// Embeds the app icon into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/DCX.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("packaging/windows/dcx.rc", embed_resource::NONE)
            .manifest_optional()
            .ok();
    }
}
