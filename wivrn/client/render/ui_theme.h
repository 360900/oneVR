/*
 * WiVRn VR streaming
 * Copyright (C) 2026  Guillaume Meunier <guillaume.meunier@centraliens.net>
 * Copyright (C) 2026  Patrick Nicolas <patricknicolas@laposte.net>
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#pragma once

#include <imgui.h>
#include <string>
#include <vector>

namespace wivrn::ui
{

struct theme
{
	std::string name;
	std::string localized_name;

	// Accent
	ImVec4 accent;
	ImVec4 accent_hovered;
	ImVec4 accent_active;
	ImVec4 on_accent; // text/icon on an accent fill

	// Surfaces, back to front
	ImVec4 background;
	ImVec4 background_contrast; // LVRR: pills and bars of the dashboard shell, the fill of modals, menus and tooltips
	ImVec4 card;
	ImVec4 card_hovered;
	ImVec4 control; // slider rail, secondary button, input
	ImVec4 control_hovered;
	ImVec4 control_active;

	// Content
	ImVec4 text;
	ImVec4 text_muted;
	ImVec4 heading; // LVRR: card titles, the dashboard's secondary color
	ImVec4 border;

	// Semantic
	ImVec4 danger;
	ImVec4 danger_hovered;
	ImVec4 success;
	ImVec4 warning;

	// Dimming when a modal popup is visible
	ImVec4 dimming;

	// LVRR: the rest of the One design tokens
	ImVec4 sidebar;
	ImVec4 surface_alt; // hovered rows of a boxed list
	ImVec4 text_faint;  // placeholders
	ImVec4 hover;       // overlay on a hovered flat item
	ImVec4 pressed;     // overlay on a pressed flat item
	ImVec4 container;   // selection
	ImVec4 on_container;
	ImVec4 accent_soft;
	ImVec4 success_soft;
	ImVec4 warning_soft;
	ImVec4 danger_soft;

	// Shape, in pixels
	float rounding;      // buttons, inputs, sliders
	float card_rounding; // cards
	float window_rounding;
	float small_rounding; // chips, check boxes
	float border_size;

	float font_scale; // global text size multiplier

	// packed color with the current global alpha applied, so draw-list widgets dim with BeginDisabled()
	ImU32 col(const ImVec4 & c) const
	{
		ImVec4 v = c;
		v.w *= ImGui::GetStyle().Alpha;
		return ImGui::ColorConvertFloat4ToU32(v);
	}

	// push colors and metrics into the global ImGui style
	void apply() const;
};

// fixed design tokens shared by the widgets
namespace metrics
{
// LVRR: the lobby is drawn at 1.4x the dashboard's logical pixels, the One spec's sizes are in
// those pixels
constexpr float one = 1.4;

constexpr float font_base = 14 * one / 30;   // user-facing 100% text scale: One's 14 px body text
constexpr float font_title = 22. / 14;       // page header, x base font size
constexpr float font_description = 12. / 14; // caption: secondary text, x base font size
constexpr float font_heading = 15. / 14;     // headerbar title, card titles

constexpr ImVec2 button_padding = {16 * one, 0};
constexpr ImVec2 card_padding = {16 * one, 14 * one};    // regular cards
constexpr ImVec2 list_card_padding = {6 * one, 6 * one}; // cards holding a list of rows

// shared height of every full-width control so they line up in a setting row, x frame height
// LVRR: the widgets use control_size below instead
constexpr float control_height = 1.65;
constexpr float control_size = 36 * one;

constexpr float toggle_aspect = 46. / 26;    // track width / height
constexpr float toggle_track = 26 * one;     // track height
constexpr float toggle_knob_inset = 3 * one; // gap between track edge and knob

constexpr float slider_track = 4 * one;
constexpr float slider_thumb = 20 * one;

constexpr float segmented_inset = 3 * one; // gap around the active segment

constexpr float icon_button_glyph = 16. / 36;  // glyph height as a fraction of the button side
constexpr float button_label_glyph = 16. / 14; // leading icon size in a labeled button, x base text size

constexpr float combo_row_height = 44 * one;    // rows in the modal
constexpr float combo_chevron = 8 * one;        // chevron size in the closed box
constexpr float combo_modal_min_width = 460;    // popup never narrower than this
constexpr ImVec2 combo_padding = {12 * one, 0}; // horizontal inner padding of box and rows

constexpr float list_row_box = 32 * one;            // leading icon box size in a list row
constexpr float list_row_box_large = 80;            // min thumbnail size for full-height list rows
constexpr float list_row_pad = 14 * one;            // horizontal inner padding of a list row
constexpr float list_row_height = 52 * one;         // minimum height of a list row
constexpr ImVec2 chip_padding = {8 * one, 3 * one}; // inner padding of a chip/badge
constexpr float chip_pill_padding_x = 12 * one;     // roomier horizontal padding for bar chips
constexpr float input_padding_x = 12 * one;         // horizontal text padding inside inputs
constexpr float font_modal_title = 18. / 14;        // modal heading, x base font size

constexpr float label_line_gap = 2 * one; // title to description
constexpr float label_bottom_pad = 6;     // breathing room below the description
constexpr float nav_section_gap = 18;     // space above a sidebar section header

constexpr ImVec2 card_item_spacing = {8 * one, 8 * one}; // ItemSpacing pushed around a card section
constexpr float setting_control_width = 480;             // width of the control in a setting row

constexpr float focus_ring = 2 * one; // border of a focused entry
constexpr float popup_margin = 40;    // room around a popup for its shadow, inside its window

// application shell: top bar, sidebar, content panel, shared by lobby and in-stream window
constexpr float sidebar_width = 300;
constexpr float top_bar_height = 72;
constexpr float content_margin = 20; // gap around the main content panel

// LVRR: the lobby shell, a One window: sidebar, headerbar, content on the canvas
constexpr float shell_sidebar_width = 220 * one;
constexpr float shell_sidebar_pad = 8 * one; // around the nav rows
constexpr float shell_nav_row = 40 * one;    // nav row height
constexpr float shell_nav_glyph = 16 * one;  // icon in a nav row
constexpr float shell_header_height = 50 * one;
constexpr float shell_close_size = 24 * one;  // round close button in the headerbar
constexpr float shell_close_glyph = 14 * one; // its icon's font size, the cross is half of it
constexpr ImVec2 shell_content_padding = {32 * one, 16 * one};
constexpr float shell_content_max_width = 640 * one; // content column, centered like the renders
constexpr float card_title_gap = 8 * one;            // heading row to its box

// application launcher grid
constexpr float app_icon_small = 120;
constexpr float app_icon_medium = 176;
constexpr float app_icon_large = 240;
constexpr float app_tile_margin = 24;        // horizontal slack each side of the icon in a tile
constexpr float app_view_toggle_width = 150; // grid/list segmented control
constexpr float app_size_toggle_width = 300; // small/medium/large segmented control
constexpr float app_spinner_radius = 110;    // app-starting spinner
constexpr float app_spinner_thickness = 22;

// modal/popup windows not yet on the card system (first-run, confirm dialogs, loading popup)
constexpr ImVec2 window_padding = {20, 20};
constexpr float window_rounding = 10;
constexpr float window_border_size = 2;

constexpr ImVec2 tooltip_padding = {10 * one, 6 * one}; // hover tooltip window padding
constexpr ImVec2 button_size = {220, 80};               // big touch buttons in confirm dialogs

// PIN entry keypad
constexpr ImVec2 pin_entry_key_size = {90, 70};
constexpr ImVec2 pin_entry_item_spacing = {10, 10};
constexpr float pin_entry_popup_width = 3 * pin_entry_key_size.x + 2 * pin_entry_item_spacing.x;
} // namespace metrics

// theme in effect, call current().apply() after mutating
theme & current();

// LVRR: the bold and semibold faces of the UI font, set by the imgui context that owns them,
// null when none
ImFont *& bold_font();
ImFont *& semibold_font();
void set_theme(const theme & t);

// panel and card background opacity, 0..1, kept out of the presets so switching palette keeps it
float & background_alpha();

std::vector<theme> presets();

struct accent_swatch
{
	const char * name;
	ImVec4 base;
	ImVec4 hovered;
	ImVec4 active;
};
std::vector<accent_swatch> accent_swatches();
void set_accent(const accent_swatch & swatch);

} // namespace wivrn::ui
