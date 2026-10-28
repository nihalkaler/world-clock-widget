// Embeds the exe icon and application manifest on Windows builds.
fn main() {
    println!("cargo:rerun-if-changed=resources");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // If no resource compiler is installed the exe still builds; it just
        // has no icon and Windows draws its dialogs in the old style.
        embed_resource::compile("resources/world-clock.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
