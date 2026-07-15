#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    netdisk_monitor_lib::run();
}
