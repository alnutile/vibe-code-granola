fn main() {
    // ScreenCaptureKit reaches Apple's frameworks through a Swift bridge, so every
    // binary we produce needs the Swift runtime (libswift_Concurrency and friends).
    // Those resolve from the dyld shared cache under /usr/lib/swift, which is not
    // on the default rpath — without this the app dies at startup with
    // "Library not loaded: @rpath/libswift_Concurrency.dylib".
    //
    // This lives in build.rs rather than .cargo/config.toml because cargo reads
    // that file relative to the *current working directory*: building from the
    // repo root instead of src-tauri would silently skip it and produce a binary
    // that cannot launch. A build script travels with the crate either way.
    #[cfg(target_os = "macos")]
    println!("cargo::rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    tauri_build::build()
}
