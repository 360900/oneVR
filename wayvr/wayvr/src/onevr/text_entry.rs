//! oneVR: text entries apps ask for over IPC, as xrizer does for OpenVR's `ShowKeyboard`. While
//! one is open the keyboard types into it instead of the focused window or the dashboard, and
//! the keyboard's top bar shows its text.

use wayvr_ipc::{
    packet_client::WlxKeyboardShowParams,
    packet_server::{PacketServer, WlxKeyboardInput},
};

use crate::{
    backend::task::{OverlayTask, TaskType, ToggleMode},
    overlays::keyboard::KEYBOARD_NAME,
    state::AppState,
    windowing::OverlaySelector,
};

/// The most characters of the text the top bar shows, its end
const SHOWN_CHARS: usize = 44;

pub struct TextEntry {
    /// The IPC connection that asked for it
    connection: u64,
    params: WlxKeyboardShowParams,
    text: String,
    /// The keyboard was hidden before and hides again after
    hide_keyboard: bool,
}

/// How a text entry ends
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Done,
    Cancel,
}

/// What typing did to a text entry
#[derive(Debug, Default, PartialEq, Eq)]
struct Typed {
    /// The keys for the app, empty when nothing changed
    key: String,
    end: Option<End>,
}

impl TextEntry {
    fn new(connection: u64, params: WlxKeyboardShowParams, hide_keyboard: bool) -> Self {
        let text = limit_chars(&params.text, params.max_chars).to_string();
        Self {
            connection,
            params,
            text,
            hide_keyboard,
        }
    }

    /// Types `typed`, the text of a key (see `XkbKeymap::text_for_key`): characters, backspace,
    /// enter and escape. Other control characters and what's over the limit are left out.
    fn type_text(&mut self, typed: &str) -> Typed {
        let mut result = Typed::default();
        for c in typed.chars() {
            match c {
                '\x08' => {
                    // the app keeps its own text in minimal mode
                    if self.text.pop().is_some() || self.params.minimal {
                        result.key.push(c);
                    }
                }
                '\r' | '\n' if self.params.multiline => self.insert('\n', &mut result.key),
                '\r' | '\n' if self.params.done_key => {
                    result.end = Some(End::Done);
                    break;
                }
                '\r' | '\n' if self.params.minimal => result.key.push('\n'),
                '\x1b' => {
                    result.end = Some(End::Cancel);
                    break;
                }
                c if c.is_control() => {}
                c => self.insert(c, &mut result.key),
            }
        }
        result
    }

    fn insert(&mut self, c: char, key: &mut String) {
        let max = self.params.max_chars as usize;
        if max == 0 || self.text.chars().count() < max {
            self.text.push(c);
            key.push(c);
        }
    }

    /// The text as the top bar shows it: masked for passwords, new lines as arrows, its end
    fn shown_text(&self) -> String {
        let shown: Vec<char> = if self.params.password {
            self.text.chars().map(|_| '•').collect()
        } else {
            self.text
                .chars()
                .map(|c| if c == '\n' { '↵' } else { c })
                .collect()
        };
        if shown.len() > SHOWN_CHARS {
            std::iter::once('…')
                .chain(shown[shown.len() - (SHOWN_CHARS - 1)..].iter().copied())
                .collect()
        } else {
            shown.into_iter().collect()
        }
    }
}

/// `text` cut to at most `max` characters, 0 being no limit
fn limit_chars(text: &str, max: u32) -> &str {
    match text.char_indices().nth(max as usize) {
        Some((end, _)) if max > 0 => &text[..end],
        _ => text,
    }
}

/// What the keyboard's top bar shows, None for the usual bar
pub struct Shown {
    pub description: String,
    pub text: String,
    pub done_key: bool,
}

pub fn shown(app: &AppState) -> Option<Shown> {
    let entry = app.onevr.text_entry.as_ref()?;
    (!entry.params.minimal).then(|| Shown {
        description: entry.params.description.clone(),
        text: entry.shown_text(),
        done_key: entry.params.done_key,
    })
}

pub const fn is_open(app: &AppState) -> bool {
    app.onevr.text_entry.is_some()
}

/// Opens a text entry for `connection` and shows the keyboard, or says why not
pub fn show(
    app: &mut AppState,
    connection: u64,
    params: WlxKeyboardShowParams,
    keyboard_visible: bool,
) -> Result<(), String> {
    if app.onevr.text_entry.is_some() {
        return Err("the keyboard is in use".into());
    }
    if app.hid_provider.keymap.is_none() {
        return Err("the keyboard has no keymap".into());
    }
    log::debug!("Text entry for IPC connection {connection}: {params:?}");
    app.onevr.text_entry = Some(TextEntry::new(connection, params, !keyboard_visible));
    changed(app);
    if !keyboard_visible {
        set_keyboard_visible(app, true);
    }
    Ok(())
}

/// Types a key's text into the open text entry
pub fn type_text(app: &mut AppState, text: &str) {
    let Some(entry) = &mut app.onevr.text_entry else {
        return;
    };
    let typed = entry.type_text(text);
    let connection = entry.connection;
    if !typed.key.is_empty() {
        let packet = PacketServer::WlxKeyboardInput(WlxKeyboardInput {
            key: typed.key,
            text: entry.text.clone(),
        });
        app.ipc_server.send_response(connection, &packet);
        changed(app);
    }
    if let Some(end) = typed.end {
        finish(app, end);
    }
}

/// Ends the open text entry with its Done key, or closes it
pub fn finish(app: &mut AppState, end: End) {
    let Some(entry) = &app.onevr.text_entry else {
        return;
    };
    // without a Done key, it can only be closed
    if end == End::Done && entry.params.done_key {
        let connection = entry.connection;
        app.ipc_server
            .send_response(connection, &PacketServer::WlxKeyboardDone);
    }
    close(app, true);
}

/// The app closed its text entry
pub fn hide(app: &mut AppState, connection: u64) {
    if app
        .onevr
        .text_entry
        .as_ref()
        .is_some_and(|e| e.connection == connection)
    {
        close(app, true);
    }
}

/// The app's connection is gone, and its text entry with it
pub fn disconnected(app: &mut AppState, connection: u64) {
    if app
        .onevr
        .text_entry
        .as_ref()
        .is_some_and(|e| e.connection == connection)
    {
        close(app, false);
    }
}

/// The keyboard was hidden, which closes the text entry
pub fn keyboard_hidden(app: &mut AppState) {
    if let Some(entry) = &mut app.onevr.text_entry {
        entry.hide_keyboard = false;
        close(app, true);
    }
}

fn close(app: &mut AppState, notify: bool) {
    let Some(entry) = app.onevr.text_entry.take() else {
        return;
    };
    if notify {
        app.ipc_server
            .send_response(entry.connection, &PacketServer::WlxKeyboardClosed);
    }
    changed(app);
    if entry.hide_keyboard {
        set_keyboard_visible(app, false);
    }
}

/// The top bar shows it next frame
const fn changed(app: &mut AppState) {
    app.onevr.text_entry_gen = app.onevr.text_entry_gen.wrapping_add(1);
}

fn set_keyboard_visible(app: &mut AppState, visible: bool) {
    app.tasks
        .enqueue(TaskType::Overlay(OverlayTask::ToggleOverlay(
            OverlaySelector::Name(KEYBOARD_NAME.into()),
            if visible {
                ToggleMode::EnsureOn
            } else {
                ToggleMode::EnsureOff
            },
        )));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(params: WlxKeyboardShowParams) -> TextEntry {
        TextEntry::new(0, params, false)
    }

    fn typed(key: &str, end: Option<End>) -> Typed {
        Typed {
            key: key.into(),
            end,
        }
    }

    #[test]
    fn typing() {
        let mut e = entry(WlxKeyboardShowParams {
            text: "ab".into(),
            done_key: true,
            ..Default::default()
        });
        assert_eq!(e.type_text("c"), typed("c", None));
        assert_eq!(e.type_text("\x08"), typed("\x08", None));
        assert_eq!(e.text, "ab");
        // tab, delete and the like type nothing
        assert_eq!(e.type_text("\t\x7f"), typed("", None));
        assert_eq!(e.type_text("\r"), typed("", Some(End::Done)));
        assert_eq!(e.type_text("\x1b"), typed("", Some(End::Cancel)));

        e.text.clear();
        assert_eq!(e.type_text("\x08"), typed("", None));
    }

    #[test]
    fn enter() {
        let mut multiline = entry(WlxKeyboardShowParams {
            multiline: true,
            done_key: true,
            ..Default::default()
        });
        assert_eq!(multiline.type_text("\r"), typed("\n", None));
        assert_eq!(multiline.text, "\n");

        let mut minimal = entry(WlxKeyboardShowParams {
            minimal: true,
            ..Default::default()
        });
        assert_eq!(minimal.type_text("\r"), typed("\n", None));
        assert_eq!(minimal.text, "");
        // its app keeps the text
        assert_eq!(minimal.type_text("\x08"), typed("\x08", None));

        let mut no_done = entry(WlxKeyboardShowParams::default());
        assert_eq!(no_done.type_text("\r"), typed("", None));
    }

    #[test]
    fn limits() {
        let mut e = entry(WlxKeyboardShowParams {
            text: "abcdef".into(),
            max_chars: 4,
            ..Default::default()
        });
        assert_eq!(e.text, "abcd");
        assert_eq!(e.type_text("x"), typed("", None));
        assert_eq!(e.type_text("\x08xy"), typed("\x08x", None));
        assert_eq!(e.text, "abcx");
        assert_eq!(limit_chars("ééé", 2), "éé");
        assert_eq!(limit_chars("abc", 0), "abc");
    }

    /// The packets as xrizer's `keyboard/wayvr.rs` sends and reads them
    #[test]
    fn packets_match_xrizer() {
        use serde_json::{from_value, json, to_value};
        use wayvr_ipc::packet_client::PacketClient;

        let show = from_value::<PacketClient>(json!({ "WlxKeyboardShow": [1, {
            "description": "Name",
            "text": "Adventurer",
            "max_chars": 32,
            "password": false,
            "multiline": false,
            "minimal": true,
            "done_key": true,
        }]}))
        .unwrap();
        let PacketClient::WlxKeyboardShow(1, params) = show else {
            panic!("{show:?}");
        };
        assert_eq!(params.text, "Adventurer");
        assert!(params.minimal && params.done_key && params.max_chars == 32);
        assert!(matches!(
            from_value(json!("WlxKeyboardHide")).unwrap(),
            PacketClient::WlxKeyboardHide
        ));

        let packets = [
            (
                PacketServer::WlxKeyboardShowResponse(1, Ok(())),
                json!({ "WlxKeyboardShowResponse": [1, { "Ok": null }] }),
            ),
            (
                PacketServer::WlxKeyboardShowResponse(1, Err("no".into())),
                json!({ "WlxKeyboardShowResponse": [1, { "Err": "no" }] }),
            ),
            (
                PacketServer::WlxKeyboardInput(WlxKeyboardInput {
                    key: "a".into(),
                    text: "ba".into(),
                }),
                json!({ "WlxKeyboardInput": { "key": "a", "text": "ba" } }),
            ),
            (PacketServer::WlxKeyboardDone, json!("WlxKeyboardDone")),
            (PacketServer::WlxKeyboardClosed, json!("WlxKeyboardClosed")),
        ];
        for (packet, expected) in packets {
            assert_eq!(to_value(&packet).unwrap(), expected);
        }
    }

    #[test]
    fn shown() {
        let mut e = entry(WlxKeyboardShowParams {
            text: "pw\nx".into(),
            ..Default::default()
        });
        assert_eq!(e.shown_text(), "pw↵x");
        e.params.password = true;
        assert_eq!(e.shown_text(), "••••");
        e.params.password = false;
        e.text = "a".repeat(SHOWN_CHARS) + "b";
        let shown = e.shown_text();
        assert_eq!(shown.chars().count(), SHOWN_CHARS);
        assert!(shown.starts_with('…') && shown.ends_with("ab"));
    }
}
