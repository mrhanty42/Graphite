fn main() {
    let package = "dev_graphite_host";
    let class = "NativeBridge";
    let methods = [
        "graphiteInit",
        "graphiteTick",
        "graphiteShutdown",
        "graphiteDebugInfo",
        "graphiteReloadMod",
        "graphiteGetDirectBufferAddress",
    ];

    println!("cargo:warning=Expected JNI symbols:");
    for method in methods {
        println!("cargo:warning=  Java_{}_{}_{}", package, class, method);
    }
}
