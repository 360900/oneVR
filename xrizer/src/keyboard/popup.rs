//! The desktop keyboard: a text entry window in its own process, `xrizer-keyboard` next to this
//! library. It takes the request as JSON on standard input and prints a reply per line.

use super::Hide;
use super::protocol::{Reply, Request};
use log::{debug, warn};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub const HELPER: &str = "xrizer-keyboard";

/// Runs a text entry in the window until it's closed. An error if the window couldn't be shown.
pub fn run(request: &Request, replies: &Sender<Reply>, hide: &Hide) -> io::Result<()> {
    let helper = crate::own_library()
        .and_then(|library| Some(library.parent()?.join(HELPER)))
        .ok_or_else(|| io::Error::other("this library's directory is unknown"))?;
    let mut command = Command::new(&helper);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: close_range and prctl are async-signal-safe. The game's descriptors (sockets, the
    // runtime's connection) aren't the window's to keep open, and the window goes with the game,
    // or with this thread, which waits for it.
    unsafe {
        command.pre_exec(|| {
            libc::syscall(libc::SYS_close_range, 3, u32::MAX, 0);
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", helper.display())))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&serde_json::to_vec(request)?)?;
    }
    let child = Arc::new(Mutex::new(child));

    let to_kill = child.clone();
    if !hide.on_hide(move || {
        let _ = to_kill.lock().unwrap().kill();
    }) {
        let _ = child.lock().unwrap().kill();
    }
    debug!("Showing the desktop keyboard");

    let mut replied = false;
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        match serde_json::from_str(&line) {
            Ok(reply) => {
                replied = true;
                if replies.send(reply).is_err() {
                    break;
                }
            }
            Err(e) => warn!("Unexpected line from {HELPER}: {e}"),
        }
    }
    let status = child.lock().unwrap().wait()?;
    // A window that failed to open says nothing; one closed by the user or by hide may not either.
    if !status.success() && !replied && !hide.is_hidden() {
        return Err(io::Error::other(format!("{HELPER} failed: {status}")));
    }
    Ok(())
}
