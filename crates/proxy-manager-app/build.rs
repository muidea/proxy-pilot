fn main() {
    println!("cargo:rerun-if-changed=assets/proxy-pilot.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/proxy-pilot.ico")
            .set("ProductName", "Proxy Pilot")
            .set("FileDescription", "Proxy Pilot")
            .compile()
            .expect("Embed Windows application icon");
    }
}
