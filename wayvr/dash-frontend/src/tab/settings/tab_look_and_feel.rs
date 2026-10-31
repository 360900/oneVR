use crate::tab::settings::{
	SettingType, SettingsMountParams, SettingsTab, UpdateExtra,
	macros::{options_category, options_checkbox, options_dropdown, options_slider_f32},
};
use crate::views::ViewUpdateParams;

pub struct State {}

impl SettingsTab for State {
	fn update(&mut self, _par: &mut ViewUpdateParams, _extra: &UpdateExtra) -> anyhow::Result<()> {
		Ok(())
	}
}

impl State {
	pub fn mount(par: SettingsMountParams) -> anyhow::Result<State> {
		let c = options_category(
			par.mp,
			par.id_parent,
			"APP_SETTINGS.LOOK_AND_FEEL",
			"@/dashboard/palette.svg",
		)?;
		// LVRR: no color palettes, gradient or rounding knobs, the UI follows oneDesign
		options_dropdown::<wlx_common::locale::Language>(par.mp, c, &SettingType::Language)?;
		options_checkbox(par.mp, c, SettingType::OpaqueBackground)?;
		// LVRR: no Hide username, LVRR's Home shows no user name
		options_checkbox(par.mp, c, SettingType::HideGrabHelp)?;
		options_slider_f32(par.mp, c, SettingType::DefaultCurvature, 0.0, 0.5, 0.05)?; // min, max, step
		options_slider_f32(par.mp, c, SettingType::DefaultOpacity, 0.05, 1.0, 0.05)?; // min, max, step
		options_slider_f32(par.mp, c, SettingType::DefaultOverlayScale, 0.7, 1.5, 0.05)?; // min, max, step
		options_dropdown::<wlx_common::config::DefaultPositioning>(par.mp, c, &SettingType::DefaultPositioning)?;
		options_slider_f32(par.mp, c, SettingType::UiAnimationSpeed, 0.5, 5.0, 0.1)?; // min, max, step
		// LVRR: the watch stays, it is the way to the dashboard
		options_checkbox(par.mp, c, SettingType::SetsOnWatch)?;
		options_checkbox(par.mp, c, SettingType::Clock12h)?;
		Ok(State {})
	}
}
