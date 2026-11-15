use crate::{
	frontend::FrontendTasks,
	tab::settings::{
		SettingType, SettingsMountParams, SettingsTab, UpdateExtra,
		macros::{options_category, options_checkbox, options_slider},
	},
	util::{popup_manager::PopupHolder, wgui_simple},
	views::{self, ViewUpdateParams, skymap_list},
};
use wgui::{assets::AssetPathRef, i18n::Translation, layout::Layout, task::Tasks};

#[derive(Clone)]
enum LocalTask {
	ShowSkymapList,
}

pub struct State {
	frontend_tasks: FrontendTasks,
	tasks: Tasks<LocalTask>,
	popup_skymap_list: PopupHolder<skymap_list::View>,
}

impl SettingsTab for State {
	fn update(&mut self, par: &mut ViewUpdateParams, _extra: &UpdateExtra) -> anyhow::Result<()> {
		self.popup_skymap_list.update(par)?;

		for task in self.tasks.drain() {
			match task {
				LocalTask::ShowSkymapList => self.show_skymap_list(par.layout),
			}
		}

		Ok(())
	}
}

impl State {
	pub fn mount(par: SettingsMountParams) -> anyhow::Result<State> {
		let popup_skymap_list = PopupHolder::<skymap_list::View>::default();
		let tasks = Tasks::<LocalTask>::new();

		let c = options_category(
			par.mp,
			par.id_parent,
			"APP_SETTINGS.APPEARANCE",
			"@/dashboard/palette.svg",
		)?;

		// Dashboard category
		{
			let c = options_category(par.mp, c, "DEFINITIONS.DASHBOARD", "@/dashboard/dashboard.svg")?;

			// oneVR: no color palettes, the UI follows oneDesign
			options_checkbox(par.mp, c, SettingType::OpaqueBackground)?;
			// oneVR: no Show username, oneVR's Home shows no user name
		}

		// Skymap category
		{
			let c = options_category(par.mp, c, "DEFINITIONS.SKYMAP", "@/dashboard/globe.svg")?;
			options_checkbox(par.mp, c, SettingType::UseSkybox)?;
			options_checkbox(par.mp, c, SettingType::UsePassthrough)?;
			// "Browse skymaps" button
			wgui_simple::create_button(wgui_simple::CreateButtonParams {
				id_parent: c,
				layout: par.mp.layout,
				content: Translation::from_translation_key("APP_SETTINGS.BROWSE_SKYMAPS"),
				icon_builtin: AssetPathRef::BuiltIn("dashboard/globe.svg"),
				on_click: tasks.get_button_click_callback(LocalTask::ShowSkymapList),
			})?;

			options_slider(par.mp, c, SettingType::GridOpacity, 0.0, 1.0, 0.05)?;
		}

		Ok(State {
			tasks,
			popup_skymap_list,
			frontend_tasks: par.frontend_tasks.clone(),
		})
	}

	fn show_skymap_list(&mut self, layout: &mut Layout) {
		views::skymap_list::mount_popup(
			self.frontend_tasks.clone(),
			layout.state.globals.clone(),
			self.popup_skymap_list.clone(),
		);
	}
}
