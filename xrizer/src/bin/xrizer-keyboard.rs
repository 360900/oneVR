//! xrizer's desktop keyboard, for games' text entries when WayVR's keyboard isn't there. Started
//! by the library.
//!
//! Usage: `xrizer-keyboard < REQUEST`, the request as JSON. Prints a JSON reply per line until the
//! window is closed. Exits with a failure if the window couldn't be shown.

#[path = "../keyboard/protocol.rs"]
mod protocol;

#[path = "../keyboard/edit.rs"]
mod edit;

#[path = "../keyboard/dialog.rs"]
mod dialog;

use std::io::{Read, Write};

fn main() {
    let mut input = Vec::new();
    let request: protocol::Request = match std::io::stdin()
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_slice(&input).map_err(|e| e.to_string()))
    {
        Ok(request) => request,
        Err(e) => {
            eprintln!("usage: xrizer-keyboard < REQUEST: {e}");
            std::process::exit(2);
        }
    };
    let shown = std::panic::catch_unwind(|| {
        dialog::run(request, |reply| {
            let mut stdout = std::io::stdout().lock();
            // The library is gone if this fails; the window is of no use then.
            let line = serde_json::to_string(&reply).expect("replies serialize");
            if writeln!(stdout, "{line}")
                .and_then(|_| stdout.flush())
                .is_err()
            {
                std::process::exit(0);
            }
        })
    });
    if shown.is_err() {
        std::process::exit(1);
    }
}
