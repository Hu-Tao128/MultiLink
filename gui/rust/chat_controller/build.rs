fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=pdh");
        println!("cargo:rustc-link-lib=iphlpapi");
        println!("cargo:rustc-link-lib=psapi");
        println!("cargo:rustc-link-lib=netapi32");
        println!("cargo:rustc-link-lib=secur32");
        println!("cargo:rustc-link-lib=propsys");
        println!("cargo:rustc-link-lib=ntdll");
    }
}
