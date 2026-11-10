//! xrizer's crash dialog in its own process, so it stays open after the crashed game exits.
//!
//! Usage: `xrizer-crash-dialog ERROR [LOG_FILE]`, with the backtrace on standard input.

#[path = "../error_dialog.rs"]
mod error_dialog;

use std::io::Read;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(error) = args.next() else {
        eprintln!("usage: xrizer-crash-dialog ERROR [LOG_FILE] < BACKTRACE");
        std::process::exit(2);
    };
    let log_file = args.next().map(PathBuf::from);
    let mut backtrace = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut backtrace);
    error_dialog::dialog(
        error.to_string_lossy().into_owned(),
        String::from_utf8_lossy(&backtrace).into_owned(),
        log_file,
    );
}
