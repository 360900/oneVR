use std::{
	cell::RefCell,
	rc::{Rc, Weak},
};
use taffy::{
	AlignItems,
	prelude::{length, percent},
};

use crate::{
	assets::AssetPathRef,
	color::{WguiColor, WguiColorName},
	components::{
		Component, ComponentBase, ComponentTrait, DestroyData, RefreshData,
		radio_group::ComponentRadioGroup,
		tooltip::{self, ComponentTooltip, TooltipTrait},
	},
	drawing,
	event::{CallbackDataCommon, EventListenerCollection, EventListenerID, EventListenerKind},
	i18n::Translation,
	layout::{self, WidgetID, WidgetPair},
	renderer_vk::text::custom_glyph::CustomGlyphData,
	renderer_vk::text::{FontWeight, TextStyle},
	sound::WguiSoundType,
	widget::{
		ConstructEssentials, EventResult,
		label::{WidgetLabel, WidgetLabelParams},
		rectangle::{WidgetRectangle, WidgetRectangleParams},
		sprite::{WidgetSprite, WidgetSpriteParams},
		util::WLength,
	},
};

pub struct Params {
	pub text: Translation,
	pub style: taffy::Style,
	pub color_checked: Option<WguiColor>,
	pub box_size: f32,
	pub checked: bool,
	pub radio_group: Option<Rc<ComponentRadioGroup>>,
	pub value: Option<Rc<str>>,
	pub tooltip: Option<tooltip::TooltipInfo>,
}

impl Default for Params {
	fn default() -> Self {
		Self {
			text: Translation::from_raw_text(""),
			style: Default::default(),
			color_checked: None,
			box_size: 20.0,
			checked: false,
			radio_group: None,
			value: None,
			tooltip: None,
		}
	}
}

pub struct CheckboxToggleEvent {
	pub checked: bool,
	pub value: Option<Rc<str>>,
}

pub type CheckboxToggleCallback = Box<dyn Fn(&mut CallbackDataCommon, CheckboxToggleEvent) -> anyhow::Result<()>>;

struct State {
	checked: bool,
	hovered: bool,
	down: bool,
	on_toggle: Option<CheckboxToggleCallback>,
	self_ref: Weak<ComponentCheckbox>,
	active_tooltip: Option<Rc<ComponentTooltip>>,
}

impl TooltipTrait for State {
	fn get(&mut self) -> &mut Option<Rc<ComponentTooltip>> {
		&mut self.active_tooltip
	}
}

#[allow(clippy::struct_field_names)]
struct Data {
	#[allow(dead_code)]
	id_container: WidgetID, // Rectangle, transparent if not hovered
	id_outer_box: WidgetID, // Rectangle, has the border
	id_inner_box: WidgetID, // Rectangle, parent of outer_box
	id_label: WidgetID,     // Label, parent of container
	value: Option<Rc<str>>, // arbitrary value assigned to the element
	radio_group: Option<Weak<ComponentRadioGroup>>,

	id_check: WidgetID, // Sprite (check mark), null for radio boxes
	color_checked: WguiColor,
}

pub struct ComponentCheckbox {
	base: ComponentBase,
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
}

const COLOR_CLEAR: WguiColor = WguiColor::Raw(drawing::Color::new(0., 0., 0., 0.));

impl ComponentTrait for ComponentCheckbox {
	fn base(&self) -> &ComponentBase {
		&self.base
	}

	fn base_mut(&mut self) -> &mut ComponentBase {
		&mut self.base
	}

	fn refresh(&self, _data: &mut RefreshData) {
		// nothing to do
	}

	fn destroy(&self, data: &mut DestroyData) {
		if let Some(comp) = self.state.borrow_mut().active_tooltip.take() {
			comp.destroy(data);
			data.destroy_widgets.push(comp.base().id);
		}
	}
}

// oneDesign check / radio: hairline-strong outline when off, filled accent with a white mark when on
fn set_box_checked(widgets: &layout::WidgetMap, data: &Data, checked: bool, hovered: bool) {
	let fill = if checked {
		if hovered {
			WguiColorName::AccentHover.into()
		} else {
			data.color_checked
		}
	} else if hovered {
		WguiColorName::Hover.into()
	} else {
		COLOR_CLEAR
	};
	let border = if checked {
		fill
	} else if hovered {
		WguiColorName::Text3.into()
	} else {
		WguiColorName::StrokeStrong.into()
	};
	let mark = if checked {
		WguiColorName::OnAccent.into()
	} else {
		COLOR_CLEAR
	};

	widgets.call(data.id_outer_box, |rect: &mut WidgetRectangle| {
		rect.params.color = fill;
		rect.params.border_color = border;
	});
	widgets.call(data.id_inner_box, |rect: &mut WidgetRectangle| {
		rect.params.color = if data.radio_group.is_some() { mark } else { COLOR_CLEAR };
	});
	widgets.call(data.id_check, |sprite: &mut WidgetSprite| {
		sprite.params.color = Some(mark);
	});
}

impl ComponentCheckbox {
	pub fn set_text(&self, common: &mut CallbackDataCommon, text: Translation) {
		let Some(mut label) = common.state.widgets.get_as::<WidgetLabel>(self.data.id_label) else {
			return;
		};

		label.set_text(common, text);
	}

	pub fn set_checked(&self, common: &mut CallbackDataCommon, checked: bool) {
		let hovered;
		{
			let mut state = self.state.borrow_mut();
			if state.checked == checked {
				return;
			}
			state.checked = checked;
			hovered = state.hovered;
		}
		set_box_checked(&common.state.widgets, &self.data, checked, hovered);
		common.alterables.mark_redraw();
	}

	pub fn get_checked(&self) -> bool {
		let state = self.state.borrow_mut();
		state.checked
	}

	pub fn get_value(&self) -> Option<Rc<str>> {
		self.data.value.clone()
	}

	/// Set checked state without triggering visual changes.
	pub(super) fn set_checked_internal(&self, checked: bool) {
		self.state.borrow_mut().checked = checked;
	}

	pub fn on_toggle(&self, func: CheckboxToggleCallback) {
		self.state.borrow_mut().on_toggle = Some(func);
	}
}

fn register_event_mouse_enter(
	state: Rc<RefCell<State>>,
	data: Rc<Data>,
	listeners: &mut EventListenerCollection,
	tooltip_info: Option<tooltip::TooltipInfo>,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseEnter,
		Box::new(move |common, _event_data, (), ()| {
			common.alterables.trigger_haptics();

			ComponentTooltip::register_hover_in(common, &tooltip_info, data.id_container, state.clone());

			let checked = {
				let mut state = state.borrow_mut();
				state.hovered = true;
				state.checked
			};

			set_box_checked(&common.state.widgets, &data, checked, true);
			common.alterables.mark_redraw();

			Ok(EventResult::Pass)
		}),
	)
}

fn register_event_mouse_leave(
	state: Rc<RefCell<State>>,
	data: Rc<Data>,
	listeners: &mut EventListenerCollection,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseLeave,
		Box::new(move |common, _event_data, (), ()| {
			common.alterables.trigger_haptics();

			let checked = {
				let mut state = state.borrow_mut();
				state.hovered = false;
				state.active_tooltip = None;
				state.checked
			};

			set_box_checked(&common.state.widgets, &data, checked, false);
			common.alterables.mark_redraw();

			Ok(EventResult::Pass)
		}),
	)
}

fn register_event_mouse_cancel(state: Rc<RefCell<State>>, listeners: &mut EventListenerCollection) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseCancel,
		Box::new(move |_common, _event_data, (), ()| {
			let mut state = state.borrow_mut();
			state.down = false;
			Ok(EventResult::Pass)
		}),
	)
}

fn register_event_mouse_press(state: Rc<RefCell<State>>, listeners: &mut EventListenerCollection) -> EventListenerID {
	listeners.register(
		EventListenerKind::MousePress,
		Box::new(move |common, _event_data, (), ()| {
			let mut state = state.borrow_mut();

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();
			common.alterables.unfocus();

			if state.hovered {
				state.down = true;
				Ok(EventResult::Consumed)
			} else {
				Ok(EventResult::Pass)
			}
		}),
	)
}

fn register_event_mouse_release(
	data: Rc<Data>,
	state: Rc<RefCell<State>>,
	listeners: &mut EventListenerCollection,
) -> EventListenerID {
	listeners.register(
		EventListenerKind::MouseRelease,
		Box::new(move |common, _event_data, (), ()| {
			let mut state = state.borrow_mut();
			let was_down = state.down;

			common.alterables.trigger_haptics();
			common.alterables.mark_redraw();

			if was_down {
				state.down = false;

				if let Some(self_ref) = state.self_ref.upgrade()
					&& let Some(radio) = data.radio_group.as_ref().and_then(Weak::upgrade)
				{
					radio.set_selected_internal(common, &self_ref)?;
					state.checked = true; // can't uncheck radiobox by clicking the checked box again
					common.alterables.play_sound(WguiSoundType::CheckboxCheck);
				} else {
					state.checked = !state.checked;
					common.alterables.play_sound(if state.checked {
						WguiSoundType::CheckboxCheck
					} else {
						WguiSoundType::CheckboxUncheck
					});
				}

				set_box_checked(&common.state.widgets, &data, state.checked, state.hovered);
				if state.hovered
					&& let Some(on_toggle) = &state.on_toggle
				{
					on_toggle(
						common,
						CheckboxToggleEvent {
							checked: state.checked,
							value: data.value.clone(),
						},
					)?;
				}
				Ok(EventResult::Consumed)
			} else {
				Ok(EventResult::Pass)
			}
		}),
	)
}

pub fn construct(ess: &mut ConstructEssentials, params: Params) -> anyhow::Result<(WidgetPair, Rc<ComponentCheckbox>)> {
	let mut style = params.style;

	// force-override style
	style.flex_wrap = taffy::FlexWrap::NoWrap;
	style.align_items = Some(AlignItems::CENTER);

	// make checkbox interaction box larger by setting padding and negative margin
	style.padding = taffy::Rect {
		left: length(4.0_f32),
		right: length(8.0_f32),
		top: length(4.0_f32),
		bottom: length(4.0_f32),
	};

	style.margin = taffy::Rect {
		left: length(-4.0_f32),
		right: length(-8.0_f32),
		top: length(-4.0_f32),
		bottom: length(-4.0_f32),
	};
	//style.align_self = Some(taffy::AlignSelf::Start); // do not stretch self to the parent
	style.gap = length(8.0_f32);

	let is_radio = params.radio_group.is_some();
	let round_box = if is_radio {
		WLength::Percent(1.0)
	} else {
		WLength::Units(6.0)
	};

	let color_checked = params.color_checked.unwrap_or_else(|| WguiColorName::Accent.into());

	let (root, _) = ess.layout.add_child(
		ess.parent,
		WidgetRectangle::create(WidgetRectangleParams {
			color: COLOR_CLEAR,
			round: WLength::Units(6.0),
			..Default::default()
		}),
		style,
	)?;

	let id_container = root.id;

	let box_size = taffy::Size {
		width: length(params.box_size),
		height: length(params.box_size),
	};

	let (outer_box, _) = ess.layout.add_child(
		id_container,
		WidgetRectangle::create(WidgetRectangleParams {
			border: 2.0,
			border_color: WguiColorName::StrokeStrong.into(),
			round: round_box,
			color: COLOR_CLEAR,
			..Default::default()
		}),
		taffy::Style {
			size: box_size,
			padding: taffy::Rect::length(if is_radio { params.box_size * 0.3 } else { 2.0 }),
			min_size: box_size,
			max_size: box_size,
			..Default::default()
		},
	)?;

	let (inner_box, _) = ess.layout.add_child(
		outer_box.id,
		WidgetRectangle::create(WidgetRectangleParams {
			round: WLength::Percent(1.0),
			color: COLOR_CLEAR,
			..Default::default()
		}),
		taffy::Style {
			size: taffy::Size {
				width: percent(1.0_f32),
				height: percent(1.0_f32),
			},
			..Default::default()
		},
	)?;

	let id_check = if is_radio {
		WidgetID::default()
	} else {
		let (check, _) = ess.layout.add_child(
			outer_box.id,
			WidgetSprite::create(WidgetSpriteParams {
				glyph_data: Some(CustomGlyphData::from_assets(
					&ess.layout.state.globals,
					AssetPathRef::WguiInternal("wgui/check.svg"),
				)?),
				color: Some(COLOR_CLEAR),
				..Default::default()
			}),
			taffy::Style {
				position: taffy::Position::Absolute,
				inset: taffy::Rect::length(2.0_f32),
				..Default::default()
			},
		)?;
		check.id
	};

	let widget_label = WidgetLabel::create(
		&mut ess.layout.state,
		WidgetLabelParams {
			content: params.text,
			style: TextStyle {
				weight: Some(FontWeight::Normal),
				color: Some(WguiColorName::Text.into()),
				..Default::default()
			},
			..Default::default()
		},
	);
	let (label, _node_label) = ess.layout.add_child(id_container, widget_label, Default::default())?;

	let data = Rc::new(Data {
		id_container,
		id_outer_box: outer_box.id,
		id_inner_box: inner_box.id,
		id_label: label.id,
		value: params.value,
		radio_group: params.radio_group.as_ref().map(Rc::downgrade),
		id_check,
		color_checked,
	});

	set_box_checked(&ess.layout.state.widgets, &data, params.checked, false);

	let state = Rc::new(RefCell::new(State {
		checked: params.checked,
		down: false,
		hovered: false,
		on_toggle: None,
		self_ref: Weak::new(),
		active_tooltip: None,
	}));

	let base = ComponentBase {
		id: root.id,
		lhandles: {
			let listeners = &mut root.widget.state().event_listeners;
			vec![
				register_event_mouse_enter(state.clone(), data.clone(), listeners, params.tooltip),
				register_event_mouse_leave(state.clone(), data.clone(), listeners),
				register_event_mouse_cancel(state.clone(), listeners),
				register_event_mouse_press(state.clone(), listeners),
				register_event_mouse_release(data.clone(), state.clone(), listeners),
			]
		},
	};

	let checkbox = Rc::new(ComponentCheckbox { base, data, state });

	if let Some(radio) = params.radio_group.as_ref() {
		radio.register_child(checkbox.clone(), params.checked);
		checkbox.state.borrow_mut().self_ref = Rc::downgrade(&checkbox);
	}

	ess.layout.defer_component_refresh(Component(checkbox.clone()));
	Ok((root, checkbox))
}
