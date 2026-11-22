//! OpenVR's virtual keyboard (`IVROverlay::ShowKeyboard`). WayVR's keyboard in the headset when
//! WayVR runs, a desktop window otherwise. Without either, the keyboard is done at once with the
//! existing text, so games waiting for it carry on.

#[cfg(test)]
mod edit;
#[cfg_attr(test, allow(dead_code))]
mod popup;
pub mod protocol;
#[cfg_attr(test, allow(dead_code))]
mod wayvr;

use openvr as vr;
use protocol::{Reply, Request};
use std::ffi::CString;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

/// Where a keyboard event goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The app's event queue.
    Global,
    Overlay(vr::VROverlayHandle_t),
}

/// Starts a keyboard for `request`, which reports to `replies` and drops it once it's closed.
/// `hide` closes it.
pub type Start = fn(Request, Sender<Reply>, Arc<Hide>);

/// Closes a keyboard from another thread.
#[derive(Default)]
pub struct Hide(Mutex<HideState>);

#[derive(Default)]
enum HideState {
    #[default]
    Open,
    Closer(Box<dyn FnOnce() + Send>),
    Hidden,
}

impl Hide {
    /// Sets how to close the keyboard. False if it was hidden already, which the caller then
    /// handles itself.
    pub fn on_hide(&self, close: impl FnOnce() + Send + 'static) -> bool {
        let mut state = self.0.lock().unwrap();
        if matches!(*state, HideState::Hidden) {
            return false;
        }
        *state = HideState::Closer(Box::new(close));
        true
    }

    pub fn hide(&self) {
        let state = std::mem::replace(&mut *self.0.lock().unwrap(), HideState::Hidden);
        if let HideState::Closer(close) = state {
            close();
        }
    }

    pub fn is_hidden(&self) -> bool {
        matches!(*self.0.lock().unwrap(), HideState::Hidden)
    }
}

struct Session {
    /// The overlay the keyboard is for, None for the scene app.
    overlay: Option<vr::VROverlayHandle_t>,
    user_value: u64,
    replies: Receiver<Reply>,
    hide: Arc<Hide>,
}

pub struct Keyboard {
    start: Start,
    /// What `GetKeyboardText` returns: the shown keyboard's text, or the last one's.
    text: Mutex<CString>,
    session: Mutex<Option<Session>>,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new(start)
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        if let Some(session) = self.session.get_mut().unwrap() {
            session.hide.hide();
        }
    }
}

impl Keyboard {
    pub fn new(start: Start) -> Self {
        Self {
            start,
            text: Mutex::default(),
            session: Mutex::default(),
        }
    }

    /// Shows a keyboard, returning the events to send now. The caller pumps first, so a keyboard
    /// that closed since is gone.
    pub fn show(
        &self,
        overlay: Option<vr::VROverlayHandle_t>,
        request: Request,
        user_value: u64,
    ) -> Result<Vec<(Target, vr::VREvent_t)>, vr::EVROverlayError> {
        {
            let mut session = self.session.lock().unwrap();
            if session.is_some() {
                return Err(vr::EVROverlayError::KeyboardAlreadyInUse);
            }
            *self.text.lock().unwrap() = c_string(&request.text);
            let (sender, replies) = mpsc::channel();
            let hide = Arc::<Hide>::default();
            (self.start)(request, sender, hide.clone());
            *session = Some(Session {
                overlay,
                user_value,
                replies,
                hide,
            });
        }
        let mut events = vec![(
            Target::Global,
            event(
                vr::EVREventType::KeyboardOpened_Global,
                overlay,
                user_value,
                "",
            ),
        )];
        // What a keyboard reported right away, like having none.
        events.extend(self.pump());
        Ok(events)
    }

    pub fn hide(&self) {
        if let Some(session) = &*self.session.lock().unwrap() {
            session.hide.hide();
        }
    }

    pub fn text(&self) -> CString {
        self.text.lock().unwrap().clone()
    }

    /// The events for what the keyboard reported since the last call.
    pub fn pump(&self) -> Vec<(Target, vr::VREvent_t)> {
        let mut events = Vec::new();
        let mut guard = self.session.lock().unwrap();
        let Some(session) = &*guard else {
            return events;
        };
        let target = session.overlay.map_or(Target::Global, Target::Overlay);
        let event = |ty, input: &str| event(ty, session.overlay, session.user_value, input);
        let closed =
            loop {
                match session.replies.try_recv() {
                    Ok(Reply::Input { key, text }) => {
                        *self.text.lock().unwrap() = c_string(&text);
                        events.extend(input_chunks(&key).map(|chunk| {
                            (target, event(vr::EVREventType::KeyboardCharInput, chunk))
                        }));
                    }
                    Ok(Reply::Done) => {
                        events.push((target, event(vr::EVREventType::KeyboardDone, "")));
                    }
                    Err(TryRecvError::Empty) => break false,
                    Err(TryRecvError::Disconnected) => {
                        events.push((target, event(vr::EVREventType::KeyboardClosed, "")));
                        events.push((
                            Target::Global,
                            event(vr::EVREventType::KeyboardClosed_Global, ""),
                        ));
                        break true;
                    }
                }
            };
        if closed {
            *guard = None;
        }
        events
    }
}

/// Text for a C string, without any nul in it.
fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap()
}

fn event(
    ty: vr::EVREventType,
    overlay: Option<vr::VROverlayHandle_t>,
    user_value: u64,
    input: &str,
) -> vr::VREvent_t {
    let mut new_input = [0; 8];
    for (dst, src) in new_input.iter_mut().zip(input.bytes()) {
        *dst = src as std::ffi::c_char;
    }
    vr::VREvent_t {
        eventType: ty.0,
        trackedDeviceIndex: vr::k_unTrackedDeviceIndex_Hmd,
        eventAgeSeconds: 0.0,
        data: vr::VREvent_Data_t {
            keyboard: vr::VREvent_Keyboard_t {
                cNewInput: new_input,
                uUserValue: user_value,
                overlayHandle: overlay.unwrap_or(vr::k_ulOverlayHandleInvalid),
            },
        },
    }
}

/// `key` in pieces that fit `cNewInput` (7 bytes and a nul), never splitting a character.
fn input_chunks(key: &str) -> impl Iterator<Item = &str> {
    let mut rest = key;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut end = rest.len().min(7);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (chunk, tail) = rest.split_at(end);
        rest = tail;
        Some(chunk)
    })
}

/// No keyboard to show: done with the text unchanged, if there's a Done key to press.
fn no_keyboard(request: &Request, replies: &Sender<Reply>) {
    if request.done_key {
        let _ = replies.send(Reply::Done);
    }
}

/// Tests have no keyboard and get its replies at once.
#[cfg(test)]
fn start(request: Request, replies: Sender<Reply>, _: Arc<Hide>) {
    no_keyboard(&request, &replies);
}

/// Tries WayVR's keyboard, then the desktop window, on a thread of their own, as both wait for
/// the user.
#[cfg(not(test))]
fn start(request: Request, replies: Sender<Reply>, hide: Arc<Hide>) {
    let spawned = std::thread::Builder::new()
        .name("xrizer-keyboard".into())
        .spawn(move || {
            type Run = fn(&Request, &Sender<Reply>, &Hide) -> std::io::Result<()>;
            let keyboards: [(&str, Run); 2] = [("WayVR", wayvr::run), ("desktop", popup::run)];
            for (name, run) in keyboards {
                if hide.is_hidden() {
                    return;
                }
                match run(&request, &replies, &hide) {
                    Ok(()) => return,
                    Err(e) => log::debug!("No {name} keyboard: {e}"),
                }
            }
            if !hide.is_hidden() {
                log::info!("No keyboard to show, reporting the text unchanged");
                no_keyboard(&request, &replies);
            }
        });
    if let Err(e) = spawned {
        log::warn!("Couldn't start the keyboard: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_fits_events() {
        let chunks = |s| input_chunks(s).collect::<Vec<_>>();
        assert_eq!(chunks("hello world"), ["hello w", "orld"]);
        assert_eq!(chunks("ééééé"), ["ééé", "éé"]);
        assert_eq!(chunks("\x1b[D"), ["\x1b[D"]);
        assert!(chunks("").is_empty());
    }

    #[test]
    fn hide_closes_once() {
        let hide = Hide::default();
        let (sender, receiver) = mpsc::channel();
        assert!(hide.on_hide(move || sender.send(()).unwrap()));
        assert!(!hide.is_hidden());
        hide.hide();
        hide.hide();
        assert!(hide.is_hidden());
        assert_eq!(receiver.try_iter().count(), 1);
        // Hidden before the keyboard was up: it's not to show.
        assert!(!hide.on_hide(|| unreachable!()));
    }
}
