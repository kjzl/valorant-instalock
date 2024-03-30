fn main() {
    built::write_built_file()
        .expect("Failed to acquire build-time information");
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("white.ico");
        res.compile().unwrap();
    }
}
