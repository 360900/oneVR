//! oneVR's desktop window: the dashboard's pages that work without a headset (status, pairing,
//! headset and server settings, diagnostics) and a tray icon. Closing the window keeps the tray,
//! and the server runs on either way.

mod activate;
mod app;
mod instance;
mod interface;
mod tray;

use std::{io::IsTerminal, process::ExitCode, time::Duration};

use dbus::{arg::PropMap, blocking::Connection};
use instance::Request;
use tracing_subscriber::{EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt};
use winit::event_loop::EventLoop;

const USAGE: &str = "usage: onevr-ui [--pair]

Opens oneVR's window, or the running one. --pair starts pairing a new headset.";

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
/// oneVR's icon, under the install prefix
const ICON: &str = "share/icons/hicolor/scalable/apps/io.github.onevr.svg";

pub enum UserEvent {
	/// From the tray, another launch or `onevr pair`, with the click's or the launch's activation
	/// token, empty without one
	Open(Request, String),
	/// The window closes, the tray stays
	Close,
	/// Starts oneVR's server, from the tray
	Start,
	/// Stops oneVR's server, then quits unless the window was opened meanwhile
	Stop,
	/// `onevr stop` ended
	Stopped,
	Quit,
	/// The server link has news, which the tray follows while the window is closed
	LinkChanged,
}

/// The activation token a launcher passed for raising the window, by display server
pub struct LaunchToken {
	/// `XDG_ACTIVATION_TOKEN`
	pub wayland: Option<String>,
	/// `DESKTOP_STARTUP_ID`
	pub x11: Option<String>,
}

fn init_logging() {
	tracing_subscriber::registry()
		.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
		.with(
			EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.from_env_lossy()
				.add_directive("cosmic_text=warn".parse().unwrap()),
		)
		.init();
}

/// Shows an error as a desktop notification when nobody reads stderr, like `onevr` does: launched
/// from the desktop, the log goes to the journal. Failures are ignored, the log has the text too.
pub fn notify(body: &str) {
	if std::io::stderr().is_terminal() {
		return;
	}
	let Ok(conn) = Connection::new_session() else {
		return;
	};
	let proxy = conn.with_proxy(NOTIFICATIONS, NOTIFICATIONS_PATH, Duration::from_secs(5));
	let icon = icon();
	let actions: Vec<&str> = Vec::new();
	let hints = PropMap::new();
	// app, the notification it replaces, icon, summary, body, actions, hints, the server's timeout
	let _ = proxy.method_call::<(u32,), _, _, _>(
		NOTIFICATIONS,
		"Notify",
		("oneVR", 0u32, icon.as_str(), "oneVR", body, actions, hints, -1i32),
	);
}

/// The installed oneVR icon, or "" without one
fn icon() -> String {
	onevr_common::install_prefix()
		.map(|prefix| prefix.join(ICON))
		.filter(|icon| icon.is_file())
		.map(|icon| icon.display().to_string())
		.unwrap_or_default()
}

fn main() -> ExitCode {
	let request = match std::env::args().nth(1).as_deref() {
		None => Request::Open,
		Some("--pair") => Request::Pair,
		Some("--help" | "-h") => {
			println!("{USAGE}");
			return ExitCode::SUCCESS;
		}
		Some(arg) => {
			eprintln!("onevr-ui: unknown argument {arg}\n\n{USAGE}");
			return ExitCode::FAILURE;
		}
	};
	// The launch's activation token, read once and cleared so apps started from the window,
	// `onevr start` and `onevr stop` too, don't use it
	let var = |name| std::env::var(name).ok().filter(|token| !token.is_empty());
	let token = LaunchToken {
		wayland: var("XDG_ACTIVATION_TOKEN"),
		x11: var("DESKTOP_STARTUP_ID"),
	};
	// SAFETY: the process is still single-threaded, so no thread reads the environment while it
	// changes. The instance, the link and the tray start theirs later.
	unsafe {
		std::env::remove_var("XDG_ACTIVATION_TOKEN");
		std::env::remove_var("DESKTOP_STARTUP_ID");
	}
	init_logging();

	let conn = match instance::claim() {
		Ok(Some(conn)) => Some(conn),
		Ok(None) => {
			return match instance::forward(request, &token.wayland.or(token.x11).unwrap_or_default()) {
				Ok(()) => ExitCode::SUCCESS,
				Err(e) => {
					let error = format!("The oneVR window is open but doesn't answer: {e}");
					log::error!("{error}");
					notify(&error);
					ExitCode::FAILURE
				}
			};
		}
		Err(e) => {
			log::warn!("No session bus, so a second window may open: {e}");
			None
		}
	};

	match run(request, token, conn) {
		Ok(()) => ExitCode::SUCCESS,
		Err(e) => {
			log::error!("{e:?}");
			notify(&format!("oneVR's window couldn't open: {e:#}"));
			ExitCode::FAILURE
		}
	}
}

fn run(request: Request, token: LaunchToken, conn: Option<Connection>) -> anyhow::Result<()> {
	let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
	let proxy = event_loop.create_proxy();
	if let Some(conn) = conn {
		instance::serve(conn, proxy.clone());
	}
	let mut app = app::App::new(proxy, request, token)?;
	event_loop.run_app(&mut app)?;
	// the window couldn't open, with no tray to try again from
	app.take_failure().map_or(Ok(()), Err)
}
