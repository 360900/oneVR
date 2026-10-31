//! oneVR: the link to the oneVR server, and dashboard features that need the backend.

mod perf_overlay;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use onevr_common::{
    link::OneVrLink,
    state::{
        Battery, BatteryDevice, Failure, Link, MAX_VIRTUAL_MONITORS, MessageKind, Notice,
        OneVrBackend, OneVrRequest, Reason,
    },
};
use serde::{Deserialize, Serialize};
use wgui::i18n::Translation;
use wlx_common::{
    astr_containers::AStrMapExt, config_io, dash_interface::DashInterface, overlays::ToastTopic,
};

#[cfg(feature = "wayland")]
use crate::overlays::screen::virtual_monitor;
use crate::{
    backend::{
        input::TrackedDeviceRole,
        task::{OverlayTask, TaskType},
    },
    overlays::{dashboard::DashInterfaceLive, toast::Toast},
    state::AppState,
    windowing::OverlaySelector,
};

const CONFIG_FILE: &str = "onevr.json5";

#[derive(Default)]
pub struct OneVrApp {
    pub link: Option<OneVrLink>,
    pub perf_overlay: bool,
    /// Screens copy their captured frames, from the config as it was at start; read when
    /// a capture starts
    pub copy_captured_frames: bool,
    /// The dashboard has a focused edit box, the keyboard types into it
    pub dash_text_focus: bool,
    /// Typed text for the dashboard
    pub dash_text: Vec<String>,
    pub virtual_monitors: Vec<VirtualMonitor>,
    /// Saved sizes not added yet, which the config keeps: they wait for the screens, and
    /// stay on a desktop that can't add monitors
    saved_monitors: Vec<(u32, u32)>,
    /// How this desktop adds outputs, once looked up; None when it can't
    #[cfg(feature = "wayland")]
    virtual_method: std::cell::OnceCell<Option<virtual_monitor::Method>>,
    /// The virtual monitor whose new output is being looked for, and until when
    locating: Option<(Arc<str>, Instant)>,
    /// The mouse's area changes once a removed output is gone
    extent_refresh_at: Option<Instant>,
    /// Counts output adds and removes, after which the virtual monitors read their place
    /// again
    layout_gen: u64,
    /// What the server last heard about the dashboard
    dashboard_reported: Option<bool>,
}

pub struct VirtualMonitor {
    pub name: Arc<str>,
    pub width: u32,
    pub height: u32,
    /// The compositor's output, once found
    pub output: Option<Arc<str>>,
}

/// oneVR's own WayVR settings
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct OneVrConfig {
    perf_overlay: bool,
    /// Sizes of the virtual monitors, added again at start
    virtual_monitors: Vec<(u32, u32)>,
    /// Copy each captured DMA-buf frame before the compositor draws into it again, for
    /// screens that mix frames during motion when it draws on another GPU. Off by
    /// default: the copy faults some drivers, and it needs a second graphics queue. Set
    /// by hand; saving keeps the file's value.
    copy_captured_frames: bool,
}

impl OneVrConfig {
    fn load() -> Self {
        let path = config_io::get_config_file_path(CONFIG_FILE);
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json5::from_str(&json).unwrap_or_else(|e| {
                log::warn!("Ignoring {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) -> anyhow::Result<()> {
        let path = config_io::get_config_file_path(CONFIG_FILE);
        std::fs::write(path, serde_json5::to_string(self)?)?;
        Ok(())
    }
}

pub fn start(app: &mut AppState) {
    if app.onevr.link.is_some() {
        return;
    }
    match OneVrLink::start() {
        Ok(link) => app.onevr.link = Some(link),
        Err(e) => log::error!("Could not start the oneVR server link: {e:?}"),
    }
    let config = OneVrConfig::load();
    app.onevr.perf_overlay = config.perf_overlay;
    app.onevr.copy_captured_frames = config.copy_captured_frames;
    if app.onevr.perf_overlay {
        perf_overlay::set_visible(app, true);
    }
    app.onevr.saved_monitors = config.virtual_monitors;
    if !app.onevr.saved_monitors.is_empty() {
        // once the screens exist, which tell whether this is Wayland: OpenVR starts this
        // before it makes them
        app.tasks.enqueue(TaskType::Global(Box::new(|app| {
            if virtual_monitors_supported(app) {
                for (width, height) in std::mem::take(&mut app.onevr.saved_monitors) {
                    add(app, width, height, true);
                }
            }
        })));
    }
}

fn save_config(app: &AppState) {
    // what WayVR doesn't change keeps the file's value, also when edited meanwhile
    let mut config = OneVrConfig::load();
    config.perf_overlay = app.onevr.perf_overlay;
    // a session that can't add the saved monitors leaves them saved
    config.virtual_monitors = if app.onevr.saved_monitors.is_empty() {
        // by name: start() adds them in order, each under the lowest free name, which
        // keys its saved placement and attributes
        let mut monitors: Vec<_> = app.onevr.virtual_monitors.iter().collect();
        monitors.sort_by(|a, b| a.name.cmp(&b.name));
        monitors.iter().map(|m| (m.width, m.height)).collect()
    } else {
        app.onevr.saved_monitors.clone()
    };
    if let Err(e) = config.save() {
        log::warn!("Could not save {CONFIG_FILE}: {e:?}");
    }
}

/// Batteries of the headset, controllers and trackers
pub fn batteries(app: &AppState) -> Vec<Battery> {
    app.input_state
        .devices
        .iter()
        .filter_map(|d| {
            let device = match d.role {
                TrackedDeviceRole::Hmd => BatteryDevice::Headset,
                TrackedDeviceRole::LeftHand => BatteryDevice::LeftController,
                TrackedDeviceRole::RightHand => BatteryDevice::RightController,
                TrackedDeviceRole::Tracker => BatteryDevice::Tracker,
                TrackedDeviceRole::None => return None,
            };
            Some(Battery {
                device,
                level: d.soc?,
                charging: d.charging,
            })
        })
        .collect()
}

/// Shows or hides the performance overlay, and remembers the choice
pub fn set_perf_overlay(app: &mut AppState, visible: bool) {
    if app.onevr.perf_overlay == visible {
        return;
    }
    app.onevr.perf_overlay = visible;
    perf_overlay::set_visible(app, visible);
    save_config(app);
}

/// Virtual monitors work on this desktop
pub fn virtual_monitors_supported(app: &mut AppState) -> bool {
    #[cfg(feature = "wayland")]
    {
        virtual_method(app).is_some()
    }
    #[cfg(not(feature = "wayland"))]
    {
        let _ = app;
        false
    }
}

#[cfg(feature = "wayland")]
fn virtual_method(app: &mut AppState) -> Option<virtual_monitor::Method> {
    if !app.feats.desktop_backend.is_wayland() {
        return None;
    }
    *app.onevr.virtual_method.get_or_init(|| {
        let method = virtual_monitor::Method::detect();
        log::info!("Virtual monitors: {method:?}");
        method
    })
}

/// Adds a virtual monitor, shown as a screen, and remembers it
pub fn add_virtual_monitor(app: &mut AppState, width: u32, height: u32) {
    add(app, width, height, false);
}

/// With `restore`, for the saved monitors added again at start, a failure keeps the
/// monitor and tries again instead of removing it
fn add(app: &mut AppState, width: u32, height: u32, restore: bool) {
    #[cfg(feature = "wayland")]
    {
        let Some(method) = virtual_method(app) else {
            return;
        };
        if app.onevr.virtual_monitors.len() >= MAX_VIRTUAL_MONITORS {
            return;
        }
        // oneVR-1 to oneVR-3, reusing removed ones
        let Some(name) = (1..=MAX_VIRTUAL_MONITORS)
            .map(|i| Arc::<str>::from(format!("oneVR-{i}")))
            .find(|n| app.onevr.virtual_monitors.iter().all(|m| m.name != *n))
        else {
            return;
        };
        log::info!("{name}: adding a {width}x{height} virtual monitor");
        app.onevr.virtual_monitors.push(VirtualMonitor {
            name: name.clone(),
            width,
            height,
            output: None,
        });
        virtual_monitor::spawn(app, method, name, width, height, restore);
        save_config(app);
    }
    #[cfg(not(feature = "wayland"))]
    let _ = (app, width, height, restore);
}

/// Removes a virtual monitor and its screen
pub fn remove_virtual_monitor(app: &mut AppState, name: &str) {
    end_locate_turn(app, name);
    let before = app.onevr.virtual_monitors.len();
    app.onevr.virtual_monitors.retain(|m| &*m.name != name);
    if app.onevr.virtual_monitors.len() == before {
        return;
    }
    app.tasks
        .enqueue(TaskType::Overlay(OverlayTask::DropImmediate(
            OverlaySelector::Name(name.into()),
        )));
    // the next monitor to take the name starts with default stereo and mouse transform
    app.session.config.attribs.arc_rm(name);
    app.onevr.extent_refresh_at = Some(Instant::now() + Duration::from_secs(1));
    save_config(app);
}

/// The compositor outputs that belong to virtual monitors
pub fn virtual_monitor_outputs(app: &AppState) -> Vec<Arc<str>> {
    app.onevr
        .virtual_monitors
        .iter()
        .filter_map(|m| m.output.clone())
        .collect()
}

pub fn set_virtual_monitor_output(app: &mut AppState, name: &str, output: Arc<str>) {
    if let Some(m) = app
        .onevr
        .virtual_monitors
        .iter_mut()
        .find(|m| &*m.name == name)
    {
        m.output = Some(output);
    }
    // a new output can move the auto-placed ones
    app.onevr.layout_gen += 1;
}

/// Changes when outputs were added or removed. wlroots and Hyprland then place the
/// auto-placed outputs again, oneVR's among them.
pub const fn layout_gen(app: &AppState) -> u64 {
    app.onevr.layout_gen
}

/// Lets one virtual monitor at a time add an output that only the order tells apart.
/// False while another holds the turn, unless that one was removed or is past its
/// deadline, as when it is hidden.
pub fn take_locate_turn(app: &mut AppState, name: &str, until: Instant) -> bool {
    if let Some((holder, deadline)) = &app.onevr.locating
        && **holder != *name
        && Instant::now() < *deadline
        && app.onevr.virtual_monitors.iter().any(|m| m.name == *holder)
    {
        return false;
    }
    app.onevr.locating = Some((name.into(), until));
    true
}

/// Gives the turn back once the output is found, or the monitor failed or was removed
pub fn end_locate_turn(app: &mut AppState, name: &str) {
    if app
        .onevr
        .locating
        .as_ref()
        .is_some_and(|(holder, _)| **holder == *name)
    {
        app.onevr.locating = None;
    }
}

/// Tells the server whether the dashboard is open, so the headset draws the hands over games
pub fn set_dashboard_visible(app: &mut AppState, visible: bool) {
    let Some(link) = app.onevr.link.as_mut() else {
        return;
    };
    if link.state().link != Link::Online || !link.state().session_running {
        // told again once the session is back
        app.onevr.dashboard_reported = None;
        return;
    }
    if app.onevr.dashboard_reported == Some(visible) {
        return;
    }
    app.onevr.dashboard_reported = Some(visible);
    let _ = link.request(OneVrRequest::SetDashboardVisible(visible));
}

/// Shows the server's messages, and failed requests, as toasts
pub fn tick(app: &mut AppState) {
    #[cfg(feature = "wayland")]
    if app
        .onevr
        .extent_refresh_at
        .is_some_and(|t| Instant::now() >= t)
    {
        app.onevr.extent_refresh_at = None;
        // auto-placed outputs after the removed one move into its place
        app.onevr.layout_gen += 1;
        if let Some(wl) = wlx_capture::wayland::WlxClient::new() {
            virtual_monitor::set_desktop_extent(app, &wl);
        }
    }

    // gives a game the Library started its title, also when its session comes while the
    // dashboard is hidden and doesn't update, and ends a start that failed. The failure is
    // copied, as the list borrows the app.
    let failed = app
        .onevr
        .link
        .as_ref()
        .and_then(|link| link.state().failed_start.clone());
    dash_frontend::frontend::onevr_poll_start(failed.as_ref(), || {
        DashInterfaceLive::new()
            .monado_client_list(app, true)
            .unwrap_or_default()
    });

    let notices = match app.onevr.link.as_mut() {
        Some(link) => link.poll(),
        None => return,
    };
    for notice in notices {
        let (topic, title, body) = match notice {
            Notice::Message(message) => {
                let topic = match message.kind {
                    MessageKind::Info => ToastTopic::System,
                    MessageKind::Urgent => ToastTopic::Priority,
                    MessageKind::Error => ToastTopic::Error,
                };
                let body = Translation::from_raw_text_string(message.text);
                (topic, Translation::from_raw_text("oneVR"), body)
            }
            // what failed, and why
            Notice::RequestFailed { title, failure } => {
                // the headset may still apply the change, so it isn't titled as a failure
                let (topic, title) = if failure == Failure::Known(Reason::HeadsetSlow) {
                    (ToastTopic::System, Translation::from_raw_text("oneVR"))
                } else {
                    (ToastTopic::Error, Translation::from_translation_key(title))
                };
                let body = match failure {
                    Failure::Known(reason) => Translation::from_translation_key(reason.key()),
                    Failure::Other(text) => Translation::from_raw_text_string(text),
                };
                (topic, title, body)
            }
        };
        Toast::new(topic, Some(title), body)
            .with_timeout(5.0)
            .submit(app);
    }
}
