//! oneVR additions to the dashboard frame.

use std::time::Duration;

use onevr_common::state::{Link, OneVrRequest};
use wgui::{
	components::{ComponentTrait, button::ComponentButton},
	drawing::Boundary,
	event::StyleSetRequest,
	i18n::Translation,
	layout::LayoutTask,
	parser::Fetchable,
	taffy,
};
use wlx_common::{config_io, dash_interface::MonadoClient};

use super::{Frontend, FrontendTask};

/// The page open before a session restart or Restart oneVR desktop
const REOPEN_FILE: &str = "onevr-reopen";
/// Older saved pages are from an earlier session
const REOPEN_WITHIN: Duration = Duration::from_secs(120);
use crate::{
	tab::{TabType, onevr::Page},
	views,
};

impl<T: 'static> Frontend<T> {
	pub(super) fn onevr_register_widgets(&mut self) -> anyhow::Result<()> {
		for (id, page) in [
			("btn_side_library", Page::Library),
			("btn_side_running", Page::Running),
			("btn_side_headset", Page::Headset),
			("btn_side_pc", Page::Pc),
		] {
			let button = self.state.fetch_component_as::<ComponentButton>(id)?;
			self
				.tasks
				.handle_button(&button, FrontendTask::SetTab(TabType::OneVr(page)));
			self.side_buttons.push((TabType::OneVr(page), button));
		}
		if self.interface.onevr_desktop() {
			self.onevr_desktop_layout()?;
		} else {
			self.onevr_reopen();
		}
		Ok(())
	}

	/// The desktop window has no VR pages, power menu or clock; its close button closes the window
	fn onevr_desktop_layout(&mut self) -> anyhow::Result<()> {
		let mut hidden = vec![
			self.state.get_widget_id("side_separator")?,
			self.state.get_widget_id("bottom_bar")?,
		];
		for id in [
			"btn_side_library",
			"btn_side_apps",
			"btn_side_running",
			"btn_side_settings",
		] {
			hidden.push(self.state.fetch_component_as::<ComponentButton>(id)?.base().get_id());
		}
		for id in hidden {
			self.layout.tasks.push(LayoutTask::SetWidgetStyle(
				id,
				StyleSetRequest::Display(taffy::Display::None),
			));
		}
		// in VR the flag stops the pointer ray; here a press on the title moves the window, as on empty space
		let titlebar = self.state.fetch_widget(&self.layout.state, "titlebar")?.widget;
		titlebar.state().flags.consume_mouse_events = false;
		Ok(())
	}

	/// The oneDesign window inside the transparent margin, in logical pixels: the desktop window resizes from its edges too.
	/// Events pushed to the layout keep it up to date, None until the first one.
	pub fn onevr_content_rect(&self) -> Option<Boundary> {
		let widget = self.layout.state.widgets.get(self.widgets.id_rect_window)?;
		let boundary = widget.state().data.cached_absolute_boundary;
		(boundary.area() > 0.0).then_some(boundary)
	}

	/// Opens PC > Headsets and starts pairing, for the desktop window's tray
	pub fn onevr_start_pairing(&mut self) {
		crate::tab::onevr::pc::START_PAIRING.set(true);
		self.onevr_open("pc:headsets");
	}

	/// The open page, for [`Self::onevr_open`]
	fn onevr_page(&self) -> Option<String> {
		Some(match self.current_tab.as_ref()?.get_type() {
			TabType::Home => "home".into(),
			TabType::Apps => "apps".into(),
			TabType::Settings => "settings".into(),
			TabType::OneVr(Page::Library) => "library".into(),
			TabType::OneVr(Page::Running) => "running".into(),
			TabType::OneVr(Page::Headset) => format!("headset:{}", crate::tab::onevr::section()),
			TabType::OneVr(Page::Pc) => format!("pc:{}", crate::tab::onevr::section()),
			_ => return None,
		})
	}

	/// A session restart or Restart oneVR desktop restarts the dashboard; it comes back on the same page
	pub(super) fn onevr_save_page(&self) {
		// the desktop window restarts with neither and never reopens a page
		if self.interface.onevr_desktop() {
			return;
		}
		let Some(page) = self.onevr_page() else {
			return;
		};
		if let Err(e) = std::fs::write(config_io::get_config_file_path(REOPEN_FILE), page) {
			log::warn!("Could not save the open page: {e}");
		}
	}

	fn onevr_reopen(&mut self) {
		let path = config_io::get_config_file_path(REOPEN_FILE);
		let Ok(page) = std::fs::read_to_string(&path) else {
			return;
		};
		let recent = std::fs::metadata(&path)
			.and_then(|m| m.modified())
			.ok()
			.and_then(|t| t.elapsed().ok())
			.is_some_and(|age| age < REOPEN_WITHIN);
		let _ = std::fs::remove_file(&path);
		if recent {
			self.onevr_open(page.trim());
		}
	}

	/// Opens a page by name, such as `headset:streaming`; for uidev
	pub fn onevr_open(&mut self, page: &str) {
		let (name, section) = page.split_once(':').unwrap_or((page, ""));
		let tab = match name {
			"home" => TabType::Home,
			"apps" => TabType::Apps,
			"settings" => TabType::Settings,
			"library" => TabType::OneVr(Page::Library),
			"running" => TabType::OneVr(Page::Running),
			"headset" => TabType::OneVr(Page::Headset),
			"pc" => TabType::OneVr(Page::Pc),
			_ => return,
		};
		crate::tab::onevr::headset::INITIAL_SECTION.set(crate::tab::onevr::headset::section_name(section));
		crate::tab::onevr::pc::INITIAL_SECTION.set(crate::tab::onevr::pc::section_name(section));
		self.tasks.push(FrontendTask::SetTab(tab));
	}

	pub(super) fn onevr_power_action(&mut self, action: &str) {
		match action {
			"onevr_restart_session" => self.onevr_confirm("ONEVR.RESTART_SESSION_CONFIRM", OneVrRequest::RestartSession),
			"onevr_disconnect" => self.onevr_confirm("ONEVR.DISCONNECT_CONFIRM", OneVrRequest::Disconnect),
			"onevr_quit" => self.onevr_confirm("ONEVR.QUIT_CONFIRM", OneVrRequest::Quit),
			_ => log::warn!("Unknown power action {action}"),
		}
	}

	pub(super) fn onevr_request(&mut self, data: &mut T, request: OneVrRequest) {
		if matches!(request, OneVrRequest::RestartSession) {
			self.onevr_save_page();
		}
		let result = self.interface.onevr_request(data, request);
		self.onevr_toast_refused(data, result);
	}

	pub(super) fn onevr_pair(&mut self, data: &mut T) {
		let result = self.interface.onevr_pair(data);
		self.onevr_toast_refused(data, result);
	}

	pub(super) fn onevr_start_server(&mut self, data: &mut T) {
		let result = self.interface.onevr_start_server(data);
		self.onevr_toast_error(result);
	}

	fn onevr_toast_error(&self, result: anyhow::Result<()>) {
		if let Err(e) = result {
			self
				.tasks
				.push(FrontendTask::PushToast(Translation::from_raw_text_string(
					e.to_string(),
				)));
		}
	}

	/// Why a request wasn't sent: without the oneVR server, in the words the pages use for that
	fn onevr_toast_refused(&mut self, data: &mut T, result: anyhow::Result<()>) {
		let Err(e) = result else {
			return;
		};
		// the banner's words, which differ in the desktop window, see `Ui::not_running`
		let not_running = if self.interface.onevr_desktop() {
			"ONEVR.DESKTOP.NOT_RUNNING"
		} else {
			"ONEVR.OFFLINE"
		};
		let message = match self.interface.onevr_state(data).map(|s| s.link) {
			Some(Link::Online) => Translation::from_raw_text_string(e.to_string()),
			Some(Link::Foreign) => Translation::from_translation_key("ONEVR.FOREIGN"),
			Some(Link::NoBus | Link::ServerAbsent) | None => Translation::from_translation_key(not_running),
		};
		self.tasks.push(FrontendTask::PushToast(message));
	}

	/// Sends `request` once the user confirms `message`
	pub(crate) fn onevr_confirm(&self, message: &str, request: OneVrRequest) {
		self.onevr_confirm_task(message, FrontendTask::OneVr(request));
	}

	/// Runs `task` once the user confirms `message`
	pub(crate) fn onevr_confirm_task(&self, message: &str, task: FrontendTask) {
		self.onevr_confirm_text(Translation::from_translation_key(message), task);
	}

	/// Stops app `id` once the user confirms stopping `name`, the same question on Home and Running
	pub(crate) fn onevr_confirm_stop(&self, id: i64, name: &str) {
		let Ok(id) = u32::try_from(id) else {
			return;
		};
		let text = self
			.globals
			.i18n()
			.translate_and_replace("ONEVR.RUNNING.STOP_CONFIRM", ("{NAME}", name));
		let task = FrontendTask::OneVr(OneVrRequest::StopApplication(id));
		self.onevr_confirm_text(Translation::from_raw_text_string(text), task);
	}

	/// [`Self::onevr_confirm_task`] for a message built at runtime, such as Stop's, which names the app
	fn onevr_confirm_text(&self, message: Translation, task: FrontendTask) {
		let tasks = self.tasks.clone();
		views::dialog_box::mount_popup(
			self.popup_dialog_box.clone(),
			self.tasks.clone(),
			views::dialog_box::Params {
				globals: self.globals.clone(),
				message,
				entries: views::dialog_box::get_entries_yes_no(),
				on_action_click: Box::new(move |action| {
					if action == "yes" {
						tasks.push(task.clone());
					}
				}),
			},
		);
	}
}

/// The side button lit on a page; Games, Monado and Donate have no visible one and open from Library, Running and PC
pub(super) const fn onevr_side_tab(tab: TabType) -> TabType {
	match tab {
		TabType::Games => TabType::OneVr(Page::Library),
		TabType::Monado => TabType::OneVr(Page::Running),
		TabType::Donate => TabType::OneVr(Page::Pc),
		_ => tab,
	}
}

/// Looks for the session of an app the Library started, with the sessions from `list`, so it is
/// shown under the Library's title. For WayVR's tick, which runs also while the dashboard is hidden.
/// `failed` is the OneVrState's `failed_start`: a start the server refused ends first, so the next
/// session isn't taken for it.
pub fn onevr_poll_start(failed: Option<&(u64, String)>, list: impl FnOnce() -> Vec<MonadoClient>) {
	crate::tab::onevr::started::note_failed(failed);
	crate::tab::onevr::started::poll(list);
}
