#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if remotedock::run().is_err() {
        eprintln!("RemoteDock could not start the desktop runtime.");
        std::process::exit(1);
    }
}
