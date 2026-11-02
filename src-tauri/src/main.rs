// Sin ventana negra de consola en Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dakarta_lib::run();
}
