fn main() {
    // The executable's icon and version information (Windows only).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set("ProductName", "Installateur du Liberl News");
        resource.set("FileDescription", "Installateur des patchs français du Liberl News");
        resource.compile().expect("ressources Windows");
    }
}
