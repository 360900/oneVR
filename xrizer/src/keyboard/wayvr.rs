//! WayVR's keyboard, over WayVR's IPC: length prefixed JSON on an abstract socket. oneVR's WayVR
//! takes text entries; other WayVR versions close the connection on the unknown packet.

use super::Hide;
use super::protocol::{Reply, Request};
use log::debug;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// WayVR's socket, in the abstract namespace.
const SOCKET: &[u8] = b"/tmp/wayvr_ipc.sock";
const PROTOCOL_VERSION: u32 = 6;
const MAGIC: &str = "wayvr_ipc";
/// How long WayVR may take to answer, which it does from its frame loop.
const TIMEOUT: Duration = Duration::from_secs(2);
/// The largest packet taken from WayVR; it sends nothing near this.
const MAX_PACKET: usize = 1 << 20;
const SHOW_SERIAL: u64 = 1;

/// Runs a text entry on WayVR's keyboard until it's closed. An error if WayVR didn't take it.
pub fn run(request: &Request, replies: &Sender<Reply>, hide: &Hide) -> io::Result<()> {
    let mut stream = UnixStream::connect_addr(&SocketAddr::from_abstract_name(SOCKET)?)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    send(
        &mut stream,
        &json!({ "Handshake": {
            "protocol_version": PROTOCOL_VERSION,
            "magic": MAGIC,
            "client_name": "xrizer",
        }}),
    )?;
    send(
        &mut stream,
        &json!({ "WlxKeyboardShow": [SHOW_SERIAL, request] }),
    )?;
    loop {
        let packet = receive(&mut stream)?;
        if let Some(reason) = disconnect_reason(&packet) {
            return Err(io::Error::other(format!("WayVR disconnected: {reason}")));
        }
        if let Some(result) = show_response(&packet) {
            result.map_err(|e| io::Error::other(format!("WayVR refused the keyboard: {e}")))?;
            break;
        }
    }
    debug!("Showing WayVR's keyboard");

    // Hiding closes the connection too, which ends the session should WayVR miss the packet.
    let mut closer = stream.try_clone()?;
    stream.set_read_timeout(None)?;
    if !hide.on_hide(move || {
        let _ = send(&mut closer, &json!("WlxKeyboardHide"));
        let _ = closer.shutdown(std::net::Shutdown::Both);
    }) {
        let _ = send(&mut stream, &json!("WlxKeyboardHide"));
        return Ok(());
    }
    // A closed or broken connection closes the keyboard as well.
    while let Ok(packet) = receive(&mut stream) {
        match reply(&packet) {
            Some(Ok(reply)) => {
                if replies.send(reply).is_err() {
                    break;
                }
            }
            Some(Err(Closed)) => break,
            None => {}
        }
    }
    Ok(())
}

/// The keyboard was closed.
#[derive(Debug, PartialEq)]
struct Closed;

/// What a packet from WayVR reports about the keyboard.
fn reply(packet: &Value) -> Option<Result<Reply, Closed>> {
    if disconnect_reason(packet).is_some() {
        return Some(Err(Closed));
    }
    match packet {
        Value::String(name) if name == "WlxKeyboardDone" => Some(Ok(Reply::Done)),
        Value::String(name) if name == "WlxKeyboardClosed" => Some(Err(Closed)),
        _ => {
            let input = packet.get("WlxKeyboardInput")?;
            Some(Ok(Reply::Input {
                key: input.get("key")?.as_str()?.into(),
                text: input.get("text")?.as_str()?.into(),
            }))
        }
    }
}

fn disconnect_reason(packet: &Value) -> Option<&str> {
    packet.get("Disconnect")?.get("reason")?.as_str()
}

/// The answer to our `WlxKeyboardShow`.
fn show_response(packet: &Value) -> Option<Result<(), String>> {
    let [serial, result] = packet
        .get("WlxKeyboardShowResponse")?
        .as_array()?
        .as_slice()
    else {
        return None;
    };
    if serial.as_u64() != Some(SHOW_SERIAL) {
        return None;
    }
    if result.get("Ok").is_some() {
        return Some(Ok(()));
    }
    Some(Err(result
        .get("Err")
        .and_then(Value::as_str)
        .unwrap_or("unknown reason")
        .into()))
}

fn send(stream: &mut UnixStream, packet: &Value) -> io::Result<()> {
    let data = serde_json::to_vec(packet)?;
    let len = u32::try_from(data.len()).map_err(io::Error::other)?;
    let mut bytes = len.to_be_bytes().to_vec();
    bytes.extend(data);
    stream.write_all(&bytes)
}

fn receive(stream: &mut UnixStream) -> io::Result<Value> {
    let mut len = [0; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_PACKET {
        return Err(io::Error::other(format!("{len} byte packet from WayVR")));
    }
    let mut data = vec![0; len];
    stream.read_exact(&mut data)?;
    Ok(serde_json::from_slice(&data)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_from_wayvr() {
        assert_eq!(
            reply(&json!({ "WlxKeyboardInput": { "key": "a", "text": "ba" } })),
            Some(Ok(Reply::Input {
                key: "a".into(),
                text: "ba".into()
            }))
        );
        assert_eq!(reply(&json!("WlxKeyboardDone")), Some(Ok(Reply::Done)));
        assert_eq!(reply(&json!("WlxKeyboardClosed")), Some(Err(Closed)));
        assert_eq!(
            reply(&json!({ "Disconnect": { "reason": "bye" } })),
            Some(Err(Closed))
        );
        assert_eq!(reply(&json!({ "WvrStateChanged": "DashboardShown" })), None);

        assert_eq!(
            show_response(&json!({ "WlxKeyboardShowResponse": [SHOW_SERIAL, { "Ok": null }] })),
            Some(Ok(()))
        );
        assert_eq!(
            show_response(&json!({ "WlxKeyboardShowResponse": [SHOW_SERIAL, { "Err": "no" }] })),
            Some(Err("no".into()))
        );
        assert_eq!(
            show_response(&json!({ "WlxKeyboardShowResponse": [9, { "Ok": null }] })),
            None
        );
    }

    #[test]
    fn requests_match_wayvr() {
        let request = Request {
            description: "Name".into(),
            text: "Adventurer".into(),
            max_chars: 32,
            password: false,
            multiline: false,
            minimal: true,
            done_key: true,
        };
        assert_eq!(
            json!({ "WlxKeyboardShow": [SHOW_SERIAL, request] }),
            json!({ "WlxKeyboardShow": [1, {
                "description": "Name",
                "text": "Adventurer",
                "max_chars": 32,
                "password": false,
                "multiline": false,
                "minimal": true,
                "done_key": true,
            }]})
        );
    }

    #[test]
    fn framing() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        send(&mut client, &json!("WlxKeyboardHide")).unwrap();
        let mut bytes = [0; 4 + 17];
        server.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes[..4], &17u32.to_be_bytes());
        assert_eq!(&bytes[4..], br#""WlxKeyboardHide""#);

        server.write_all(&bytes).unwrap();
        assert_eq!(receive(&mut client).unwrap(), json!("WlxKeyboardHide"));

        server
            .write_all(&(MAX_PACKET as u32 + 1).to_be_bytes())
            .unwrap();
        assert!(receive(&mut client).is_err());
    }
}
