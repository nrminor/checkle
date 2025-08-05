fn main() {
    // On Windows, set a larger stack size to match Unix systems (8MB)
    // This prevents stack overflow issues that occur with the default 1MB stack
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }
}
