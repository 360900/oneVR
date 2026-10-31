//! The dashboard's view of the system in the desktop window: the oneVR server and nothing else.
//! VR apps, overlays and the playspace only exist inside a headset session.

use std::{
	cell::RefCell,
	process::{Command, Stdio},
	rc::Rc,
	sync::mpsc::{self, Receiver, TryRecvError},
	time::{Duration, Instant},
};

use onevr_common::{
	onevr_command,
	state::{Battery, BatteryDevice, Failure, Link, Notice, OneVrBackend, OneVrRequest, OneVrState, Reason},
};
use wayvr_ipc::{
	packet_client::WvrProcessLaunchParams,
	packet_server::{WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle},
};
use wgui::i18n::Locale;
use winit::event_loop::EventLoopProxy;
use wlx_common::{
	config::GeneralConfig,
	dash_interface::{
		ConfigChangeKind, DashBackendTask, DashInterface, DashPlayspaceTask, HmdStats, InterfaceFeats, MonadoClient,
		MonadoDumpSessionFrame,
	},
	desktop_finder::DesktopFinder,
};

use crate::UserEvent;

/// How long a PIN works, like `onevr pair`'s
const PAIRING_SECS: i32 = 120;
/// How long the link gets to reach oneVR once `onevr start` started it. It asks again at least every
/// 5 s while a starting server doesn't answer.
const LINK_TIMEOUT: Duration = Duration::from_secs(10);

pub struct UiInterface {
	onevr: Box<dyn OneVrBackend>,
	desktop_finder: DesktopFinder,
	general_config: GeneralConfig,
	/// What the window shows as toasts
	toasts: Rc<RefCell<Vec<Notice>>>,
	/// oneVR starts, until the link reaches it or the start fails
	start: Option<Start>,
	proxy: EventLoopProxy<UserEvent>,
}

/// `onevr start` run by Start oneVR, or by Pair headset while oneVR was stopped
struct Start {
	/// Whether it started oneVR, once it returned
	started: Receiver<bool>,
	/// It did: when to stop waiting for the link
	deadline: Option<Instant>,
	/// Pair headset ran it or came while it ran: pairing follows once the link reaches oneVR
	pair: bool,
}

impl Start {
	/// Runs `onevr start`, which notifies its own failures, on a thread that reaps it
	fn spawn(pair: bool) -> anyhow::Result<Self> {
		let mut child = Command::new(onevr_command())
			.arg("start")
			.stdin(Stdio::null())
			.spawn()
			// not a context: the toasts and the tray's notification show to_string(), without the cause
			.map_err(|e| anyhow::anyhow!("Could not run onevr start: {e}"))
			.inspect_err(|e| log::error!("{e}"))?;
		let (tx, rx) = mpsc::channel();
		std::thread::spawn(move || {
			let _ = tx.send(child.wait().is_ok_and(|status| status.success()));
		});
		Ok(Self {
			started: rx,
			deadline: None,
			pair,
		})
	}
}

impl UiInterface {
	pub fn new(
		onevr: Box<dyn OneVrBackend>,
		locale: &Locale,
		toasts: Rc<RefCell<Vec<Notice>>>,
		proxy: EventLoopProxy<UserEvent>,
	) -> Self {
		// serde defaults, not WayVR's config: nothing here is saved. The panel is see-through
		// like in VR.
		let general_config: GeneralConfig = serde_json::from_str("{}").unwrap();
		Self {
			onevr,
			desktop_finder: DesktopFinder::new(locale),
			general_config,
			toasts,
			start: None,
			proxy,
		}
	}

	/// Ends the session if one runs, then asks for a PIN, which the link waits for
	fn pair(&mut self) -> anyhow::Result<()> {
		let state = self.onevr.state();
		// A PIN is up: it stays, with its timeout. Stop pairing, then Pair headset gives a new one.
		if state.link == Link::Online && state.pairing_enabled && !state.pin.is_empty() {
			return Ok(());
		}
		// The server refuses to pair during a session. Pair headset asked to end it, or it began as
		// oneVR started for this pairing.
		if state.session_running || state.headset_connected {
			self.onevr.request(OneVrRequest::StopSession)?;
		}
		self.onevr.request(OneVrRequest::EnablePairing {
			timeout_secs: PAIRING_SECS,
		})
	}

	/// Follows a start until the link reaches oneVR, then pairs if Pair headset asked. Returns why
	/// that pairing couldn't start.
	fn follow_start(&mut self) -> Option<Notice> {
		let Some(start) = &mut self.start else {
			return None;
		};
		let online = self.onevr.state().link == Link::Online;
		if !online {
			let failed = match start.deadline {
				Some(deadline) => Instant::now() > deadline,
				None => match start.started.try_recv() {
					Ok(true) => {
						start.deadline = Some(Instant::now() + LINK_TIMEOUT);
						false
					}
					Err(TryRecvError::Empty) => false,
					// onevr notified why
					Ok(false) | Err(TryRecvError::Disconnected) => true,
				},
			};
			if !failed {
				return None;
			}
		}
		let (pair, started) = (start.pair, start.deadline.is_some());
		self.start = None;
		if !pair {
			return None;
		}
		// told like the link's own pairing failures: "Couldn't start pairing: <why>"
		let failure = if online {
			match self.pair() {
				Ok(()) => return None,
				Err(e) => Failure::Other(e.to_string()),
			}
		} else if started {
			// `onevr start` succeeded, but the link didn't reach the server since
			Failure::Known(Reason::ServerGone)
		} else {
			Failure::Other("oneVR didn't start.".into())
		};
		Some(Notice::RequestFailed {
			title: "ONEVR.FAILED.PAIR",
			failure,
		})
	}
}

impl DashInterface<()> for UiInterface {
	fn onevr_state<'a>(&'a mut self, _: &'a mut ()) -> Option<&'a OneVrState> {
		let mut notices = self.onevr.poll();
		notices.extend(self.follow_start());
		self.toasts.borrow_mut().extend(notices);
		Some(self.onevr.state())
	}

	fn onevr_request(&mut self, _: &mut (), request: OneVrRequest) -> anyhow::Result<()> {
		self.onevr.request(request)
	}

	// only the headset's: the controllers' are read from OpenXR, in VR
	fn onevr_batteries(&mut self, _: &mut ()) -> Vec<Battery> {
		let state = self.onevr.state();
		let stats = state.latest_stats().filter(|_| state.headset_connected);
		stats
			.and_then(|s| {
				Some(Battery {
					device: BatteryDevice::Headset,
					level: s.battery?,
					charging: s.charging,
				})
			})
			.into_iter()
			.collect()
	}

	fn onevr_desktop(&self) -> bool {
		true
	}

	fn onevr_start_server(&mut self, _: &mut ()) -> anyhow::Result<()> {
		// a second one would only notify again
		if self.start.is_none() {
			self.start = Some(Start::spawn(false)?);
		}
		Ok(())
	}

	fn onevr_starting(&self) -> bool {
		self.start.is_some()
	}

	fn onevr_pair(&mut self, _: &mut ()) -> anyhow::Result<()> {
		if self.onevr_pairing() {
			return Ok(());
		}
		// with another WiVRn server running, the request fails and says why
		if matches!(self.onevr.state().link, Link::Online | Link::Foreign) {
			return self.pair();
		}
		// like `onevr pair`, which starts oneVR first, unless Start oneVR runs it already
		match &mut self.start {
			Some(start) => start.pair = true,
			// Told like a start that failed, see `follow_start`. An error would be toasted as a refused
			// request, which says that oneVR isn't running.
			None => match Start::spawn(true) {
				Ok(start) => self.start = Some(start),
				Err(e) => self.toasts.borrow_mut().push(Notice::RequestFailed {
					title: "ONEVR.FAILED.PAIR",
					failure: Failure::Other(e.to_string()),
				}),
			},
		}
		Ok(())
	}

	fn onevr_pairing(&self) -> bool {
		self.start.as_ref().is_some_and(|s| s.pair) || self.onevr.state().pairing_pending
	}

	fn clipboard_set_text(&mut self, _: &mut (), text: &str) -> anyhow::Result<()> {
		onevr_common::clipboard::set_text(text)
	}

	fn toggle_dashboard(&mut self, _: &mut ()) {
		let _ = self.proxy.send_event(UserEvent::Close);
	}

	fn general_config<'a>(&'a mut self, _: &'a mut ()) -> &'a mut GeneralConfig {
		&mut self.general_config
	}

	fn desktop_finder<'a>(&'a mut self, _: &'a mut ()) -> &'a mut DesktopFinder {
		&mut self.desktop_finder
	}

	fn get_feats(&mut self, _: &mut ()) -> InterfaceFeats {
		InterfaceFeats::default_for_backend(wlx_common::XrBackend::OpenXR)
	}

	// The rest belongs to a headset session, which the desktop window is not part of

	fn window_list(&mut self, _: &mut ()) -> anyhow::Result<Vec<WvrWindow>> {
		Ok(Vec::new())
	}

	fn window_set_visible(&mut self, _: &mut (), _: WvrWindowHandle, _: bool) -> anyhow::Result<()> {
		anyhow::bail!("No desktop windows here")
	}

	fn window_request_close(&mut self, _: &mut (), _: WvrWindowHandle) -> anyhow::Result<()> {
		anyhow::bail!("No desktop windows here")
	}

	fn process_get(&mut self, _: &mut (), _: WvrProcessHandle) -> Option<WvrProcess> {
		None
	}

	fn process_launch(&mut self, _: &mut (), _: bool, _: WvrProcessLaunchParams) -> anyhow::Result<WvrProcessHandle> {
		anyhow::bail!("Apps start in VR")
	}

	fn process_list(&mut self, _: &mut ()) -> anyhow::Result<Vec<WvrProcess>> {
		Ok(Vec::new())
	}

	fn process_terminate(&mut self, _: &mut (), _: WvrProcessHandle) -> anyhow::Result<()> {
		anyhow::bail!("Apps start in VR")
	}

	fn monado_client_list(&mut self, _: &mut (), _: bool) -> anyhow::Result<Vec<MonadoClient>> {
		Ok(Vec::new())
	}

	fn monado_client_focus(&mut self, _: &mut (), _: i64) -> anyhow::Result<()> {
		anyhow::bail!("VR apps are managed in VR")
	}

	fn monado_brightness_get(&mut self, _: &mut ()) -> Option<f32> {
		None
	}

	fn monado_brightness_set(&mut self, _: &mut (), _: f32) -> Option<()> {
		None
	}

	fn monado_metrics_set_enabled(&mut self, _: &mut (), _: bool) -> bool {
		false
	}

	fn monado_metrics_dump_session_frames(&mut self, _: &mut ()) -> Vec<MonadoDumpSessionFrame> {
		Vec::new()
	}

	fn playspace_task(&mut self, _: &mut (), _: DashPlayspaceTask) -> anyhow::Result<()> {
		anyhow::bail!("The playspace is set in VR")
	}

	fn backend_task(&mut self, _: &mut (), _: DashBackendTask) -> anyhow::Result<()> {
		anyhow::bail!("Bindings are set in VR")
	}

	fn config_changed(&mut self, _: &mut (), _: ConfigChangeKind) {}

	fn restart(&mut self, _: &mut ()) {}

	fn close(&mut self, _: &mut ()) {}

	fn hmd_stats(&mut self, _: &mut ()) -> HmdStats {
		HmdStats::default()
	}

	fn enumerate_vulkan_devices(&mut self, _: &mut ()) -> anyhow::Result<Vec<String>> {
		Ok(Vec::new())
	}
}
