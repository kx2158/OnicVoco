fn main() {
    // Embed the app icon and version info into OnicVoco.exe.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "OnicVoco");
        res.set("FileDescription", "OnicVoco - gentle voice training");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
