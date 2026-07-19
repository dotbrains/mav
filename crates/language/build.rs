fn main() {
    if let Ok(bundled) = std::env::var("MAV_BUNDLE") {
        println!("cargo:rustc-env=MAV_BUNDLE={}", bundled);
    }
}
