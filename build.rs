fn main() {
    println!("cargo:rerun-if-changed=assets/icons/netdraw.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icons/netdraw.ico")
            .set("ProductName", "NetDraw")
            .set("FileDescription", "NetDraw network diagram editor")
            .compile()
            .expect("Could not compile the NetDraw Windows icon resource");
    }
}
