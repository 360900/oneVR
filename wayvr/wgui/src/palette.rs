use std::ops::Index;

use strum::EnumCount;

use crate::{
	color::{WguiColor, WguiColorName},
	drawing,
};

#[derive(Clone)]
pub struct WguiColorPalette {
	colors: [drawing::Color; WguiColorName::COUNT],
}

impl WguiColorPalette {
	/// The oneDesign "One" light palette with blue accent
	pub fn one() -> Self {
		ONE.clone()
	}

	pub const fn from_inner(colors: [drawing::Color; WguiColorName::COUNT]) -> Self {
		Self { colors }
	}

	fn apply_modifier(color: WguiColor, modifier: &str) -> Option<WguiColor> {
		match modifier {
			"transparent" => Some(color.with_alpha(0.0)),
			"opaque" => Some(color.with_alpha(1.0)),
			other => {
				let (prefix, val_str) = other.rsplit_once('-')?;
				let val = val_str.parse::<f32>().ok()?;
				match prefix {
					"opacity" => Some(color.with_alpha(val)),
					"rgb-mult" => Some(color.mult_rgb(val)),
					"rgb-add" => Some(color.add_rgb(val)),
					_ => None,
				}
			}
		}
	}

	pub fn find(&self, in_name: &str) -> Option<WguiColor> {
		if let Some((base, modifiers_str)) = in_name.split_once('(') {
			let base_name = base.trim();
			let modifiers = modifiers_str.trim_end_matches(')');

			let name = WguiColorName::try_from(base_name).ok()?;
			let mut color = name.to_wgui_color();

			for mod_str in modifiers.split(',') {
				color = WguiColorPalette::apply_modifier(color, mod_str.trim())?;
			}

			Some(color)
		} else {
			WguiColorName::try_from(in_name).ok().map(|n| n.to_wgui_color())
		}
	}
}

impl Index<WguiColorName> for WguiColorPalette {
	type Output = drawing::Color;

	fn index(&self, name: WguiColorName) -> &Self::Output {
		// internally guaranteed to never be out of range
		&self.colors[name as usize]
	}
}

// palettes below

const fn hex(hex: &str) -> drawing::Color {
	match drawing::Color::from_hex(hex) {
		Some(color) => color,
		// just fail the build
		None => panic!("invalid static color"),
	}
}

// Alpha overlays are pre-compensated for linear-space blending (sRGB render targets) so they
// match the CSS reference on light surfaces; soft tints are pre-blended over Surface.
static ONE: &WguiColorPalette = &WguiColorPalette {
	colors: [
		hex("#e2e5e9"),   // Desktop
		hex("#f4f7fb"),   // Canvas
		hex("#eceff2"),   // Sidebar
		hex("#fcffff"),   // Surface
		hex("#f1f4f7"),   // SurfaceAlt
		hex("#f9fcff"),   // Float
		hex("#f4f7fb"),   // Headerbar
		hex("#f9fcfff0"), // Glass (near-opaque: nothing blurs behind VR panels)
		hex("#12161b"),   // Text
		hex("#51565c"),   // Text2
		hex("#81878d"),   // Text3
		hex("#00000037"), // Stroke
		hex("#00000065"), // StrokeStrong
		hex("#0000001d"), // Hover
		hex("#00000031"), // Active
		hex("#00000021"), // Fill
		hex("#00000041"), // FillStrong
		hex("#2474cf"),   // Accent
		hex("#1467c2"),   // AccentHover
		hex("#005bb5"),   // AccentPressed
		hex("#ffffff"),   // OnAccent
		hex("#1467c2"),   // AccentText
		hex("#c5e8ff"),   // Container
		hex("#003b7e"),   // OnContainer
		hex("#deebf8"),   // AccentSoft
		hex("#0e8c41"),   // Success
		hex("#ddf0e6"),   // SuccessSoft
		hex("#a96200"),   // Warning
		hex("#f1ebde"),   // WarningSoft
		hex("#d02b31"),   // Danger
		hex("#f6e4e4"),   // DangerSoft
		hex("#00000060"), // Scrim
		hex("#000000"),   // Shadow
		hex("#ffffff"),   // White
	],
};
