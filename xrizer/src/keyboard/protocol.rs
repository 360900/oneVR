//! What the keyboards are asked and what they report. Shared by the library and the
//! `xrizer-keyboard` executable, so it uses nothing else of the crate.

use serde::{Deserialize, Serialize};

/// A text entry as the app asked for it. WayVR's `WlxKeyboardShowParams` has the same fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Shown above the text.
    pub description: String,
    /// The text to start from.
    pub text: String,
    /// The most characters the text may have, 0 for no limit.
    pub max_chars: u32,
    /// The text is shown masked.
    pub password: bool,
    /// Enter types a new line instead of finishing.
    pub multiline: bool,
    /// OpenVR's minimal mode: keys go to the app as they're typed and the keyboard shows no text.
    pub minimal: bool,
    /// The keyboard has a Done key. Without one, it can only be closed.
    pub done_key: bool,
}

/// What a keyboard reports. It's closed once it reports nothing more.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    /// A key, in SteamVR's minimal mode encoding (the typed text, `"\b"` for backspace, `"\n"`
    /// for enter, `"\x1b"` for escape, `"\x1b[D"` and the like for arrows), and the whole text
    /// after it.
    Input { key: String, text: String },
    /// The Done key was pressed.
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_round_trip() {
        for reply in [
            Reply::Input {
                key: "\x08".into(),
                text: "ab".into(),
            },
            Reply::Done,
        ] {
            let json = serde_json::to_string(&reply).unwrap();
            assert_eq!(serde_json::from_str::<Reply>(&json).unwrap(), reply);
        }
    }
}
