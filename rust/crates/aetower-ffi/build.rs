fn main() {
    // Rust/LLVM post-link stripping can produce a dylib rejected by dyld on
    // macOS 27 (rust-lang/rust#157750). Let the linker produce the final compact
    // layout instead. The package release profile disables the later rewrite.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var("PROFILE").as_deref() == Ok("release")
    {
        println!("cargo::rustc-link-arg-cdylib=-Wl,-x,-S");
    }
}
