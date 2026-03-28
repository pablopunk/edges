fn main() {
    // Pass EDGES_VERSION to the binary at compile time
    if let Ok(v) = std::env::var("EDGES_VERSION") {
        println!("cargo:rustc-env=EDGES_VERSION={}", v);
    }
    println!("cargo:rerun-if-env-changed=EDGES_VERSION");

    // Link frameworks - matching JankyBorders exactly
    println!("cargo:rustc-link-lib=framework=SkyLight");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=CoreVideo");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=Foundation");
    
    // Add framework search path
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");
}