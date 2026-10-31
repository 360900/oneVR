use slotmap::Key;

use crate::{
	color::{WguiColor, WguiColorName},
	drawing::{self, GradientMode, PrimitiveExtent},
	event::CallbackDataCommon,
	globals::Globals,
	layout::WidgetID,
	widget::{WidgetStateFlags, util::WLength},
};

use super::{WidgetObj, WidgetState};

#[derive(Debug, Default)]
pub struct WidgetRectangleParams {
	pub color: WguiColor,
	pub color2: WguiColor,
	pub gradient: GradientMode,

	pub border: f32,
	pub border_color: WguiColor,

	pub round: WLength,

	/// oneDesign elevation level (0-3), drawn as a soft shadow behind the rectangle
	pub shadow: u8,
}

pub struct WidgetRectangle {
	pub params: WidgetRectangleParams,
	id: WidgetID,
}

impl WidgetRectangle {
	pub fn create(params: WidgetRectangleParams) -> WidgetState {
		WidgetState::new(
			WidgetStateFlags::default(),
			Box::new(Self {
				params,
				id: WidgetID::null(),
			}),
		)
	}

	pub fn set_color(&mut self, common: &mut CallbackDataCommon, color: WguiColor) {
		self.params.color = color;
		common.mark_widget_dirty(self.id);
	}

	pub const fn get_color(&self) -> WguiColor {
		self.params.color
	}

	pub fn set_border_color(&mut self, common: &mut CallbackDataCommon, color: WguiColor) {
		self.params.border_color = color;
		common.mark_widget_dirty(self.id);
	}

	pub const fn get_border_color(&self) -> WguiColor {
		self.params.border_color
	}
}

impl WidgetObj for WidgetRectangle {
	fn draw(&mut self, state: &mut super::DrawState, _params: &super::DrawParams) {
		let boundary = drawing::Boundary::construct_relative(state.transform_stack);

		let round_units = match self.params.round {
			WLength::Units(units) => (f32::min(boundary.size.x, boundary.size.y) as u8 / 2).min(units as u8),
			WLength::Percent(percent) => (f32::min(boundary.size.x, boundary.size.y) * percent / 2.0) as u8,
		};

		let transform = state.transform_stack.get().transform;
		let color = self.params.color.resolve(&state.globals.palette);

		if self.params.shadow > 0 {
			let mut shadow_color = state.globals.palette[WguiColorName::Shadow];
			// fade together with the fill (e.g. animated popups)
			shadow_color.a *= color.a;
			drawing::push_elevation(
				state.primitives,
				boundary,
				transform,
				f32::from(round_units),
				self.params.shadow,
				shadow_color,
			);
		}

		let color2 = self.params.color2.resolve(&state.globals.palette);
		let border_color = self.params.border_color.resolve(&state.globals.palette);

		state.primitives.push(drawing::RenderPrimitive::Rectangle(
			PrimitiveExtent { boundary, transform },
			drawing::Rectangle {
				color,
				color2,
				border_color,
				gradient: self.params.gradient,
				border: self.params.border,
				round_units,
			},
		));
	}

	fn get_id(&self) -> WidgetID {
		self.id
	}

	fn set_id(&mut self, id: WidgetID) {
		self.id = id;
	}

	fn get_type(&self) -> super::WidgetType {
		super::WidgetType::Rectangle
	}

	fn debug_print(&self, globals: &Globals) -> String {
		format!(
			"[color: {}][color2: {}][gradient: {:?}][shadow: {}]",
			self.params.color.resolve(&globals.palette).debug_ansi_block(),
			self.params.color2.resolve(&globals.palette).debug_ansi_block(),
			self.params.gradient,
			self.params.shadow,
		)
	}
}
