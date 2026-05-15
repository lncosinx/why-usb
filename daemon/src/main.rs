#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("vhci.h");

        fn init_vhci_driver() -> i32;
    }
}

fn main() {
    println!("Hello from daemon!");
    let status = ffi::init_vhci_driver();
    println!("init_vhci_driver status: {}", status);
}
