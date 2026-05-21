fn main() {
    let dst = cmake::Config::new("../driver").build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=why_usb_vhci");

    cxx_build::bridge("src/driver_backend.rs")
        .include("../driver/inc")
        .include(format!("{}/include", dst.display()))
        .flag_if_supported("-std=c++17")
        .compile("why_usb_vhci_bridge");

    println!("cargo:rerun-if-changed=src/driver_backend.rs");
    println!("cargo:rerun-if-changed=../driver/src/vhci.cpp");
    println!("cargo:rerun-if-changed=../driver/inc/vhci.h");
    println!("cargo:rerun-if-changed=../driver/inc/ioctl.h");
    println!("cargo:rerun-if-changed=../driver/CMakeLists.txt");
}
