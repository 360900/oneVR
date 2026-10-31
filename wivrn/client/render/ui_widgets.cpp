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

#include "ui_widgets.h"

#include "imgui.h"
#include "imgui_internal.h"
#include "ui_theme.h"

#include "IconsLvrr.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <functional>
#include <unordered_map>
#include <vector>

// drag-to-scroll helper, defined in imgui_impl.cpp
void ScrollWhenDragging();

namespace wivrn::ui
{

namespace
{
std::function<void()> hover_haptic_hook;
std::function<void(const char *)> tooltip_hook;

// height reserved for the pinned sidebar footer, set by begin_sidebar
float sidebar_footer_height = 0;
} // namespace

void set_hover_haptic(std::function<void()> hook)
{
	hover_haptic_hook = std::move(hook);
}

void set_tooltip_hook(std::function<void(const char *)> hook)
{
	tooltip_hook = std::move(hook);
}

// fire the hover haptic for the last submitted item, hook checks if it is hovered
static void hover_haptic()
{
	if (hover_haptic_hook)
		hover_haptic_hook();
}

// show a tooltip for the last item, caller ensures it is hovered
static void show_tooltip(const std::string & text)
{
	if (not text.empty() and tooltip_hook)
		tooltip_hook(text.c_str());
}

// alpha that darkens a light backdrop as much as black at alpha a does in sRGB, the GUI blends
// in linear light
static float ink(float a)
{
	const float c = 1 - a;
	return 1 - (c <= 0.04045f ? c / 12.92f : std::pow((c + 0.055f) / 1.055f, 2.4f));
}

// font size to push for a glyph of px pixels, PushFont scales its argument by the style
static float unscaled(float px)
{
	const ImGuiStyle & style = ImGui::GetStyle();
	return px / (style.FontScaleMain * style.FontScaleDpi);
}

static float control_size()
{
	return metrics::control_size;
}

static ImVec4 mix(const ImVec4 & a, const ImVec4 & b, float f)
{
	return {a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, a.z + (b.z - a.z) * f, a.w + (b.w - a.w) * f};
}

void shadow(ImDrawList * draw, const ImVec2 & min, const ImVec2 & max, float rounding, int level)
{
	const theme & t = current();
	struct part
	{
		float y, blur, spread, alpha;
	};
	// the One spec's box-shadows, in its logical pixels
	static const part parts[3][2] = {
	        {{1, 2, 0, 0.05f}, {0, 0, 0, 0}},
	        {{1, 2, 0, 0.06f}, {10, 28, -6, 0.18f}},
	        {{2, 6, 0, 0.07f}, {28, 70, -18, 0.36f}},
	};
	for (const part & p: parts[std::clamp(level, 1, 3) - 1])
	{
		if (p.alpha <= 0)
			continue;
		const int n = std::max(2, int(p.blur * metrics::one / 3)); // a ring every 3 px
		// stacked rings from -blur/2 to +blur/2 around the spread approximate the blur
		const float a = 1 - std::pow(1 - ink(p.alpha), 1.f / n);
		for (int i = 0; i < n; ++i)
		{
			const float e = (p.spread + p.blur * ((i + 0.5f) / n - 0.5f)) * metrics::one;
			const ImVec2 off{0, p.y * metrics::one};
			draw->AddRectFilled(min - ImVec2{e, e} + off, max + ImVec2{e, e} + off, t.col({0, 0, 0, a}), std::max(0.f, rounding + e));
		}
	}
}

void surface_frame(ImDrawList * draw, const ImVec2 & min, const ImVec2 & max, float rounding, const ImVec4 & fill, int level)
{
	const theme & t = current();
	if (level > 0)
		shadow(draw, min, max, rounding, level);
	draw->AddRectFilled(min, max, t.col(fill), rounding);
	if (t.border_size > 0)
		draw->AddRect(min, max, t.col(t.border), rounding, 0, t.border_size);
}

void push_float_style(const ImVec2 & padding)
{
	const theme & t = current();
	ImGui::PushStyleColor(ImGuiCol_PopupBg, ImVec4{0, 0, 0, 0});
	ImGui::PushStyleColor(ImGuiCol_Border, ImVec4{0, 0, 0, 0});
	ImGui::PushStyleColor(ImGuiCol_Text, t.text);
	ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, padding + ImVec2{metrics::popup_margin, metrics::popup_margin});
	ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 0);
	ImGui::PushStyleVar(ImGuiStyleVar_PopupBorderSize, 0);
}

void pop_float_style()
{
	ImGui::PopStyleVar(3);
	ImGui::PopStyleColor(3);
}

void float_frame(float rounding, int level, const ImVec4 * fill)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	const ImVec2 m{metrics::popup_margin, metrics::popup_margin};
	window->DrawList->PushClipRect(window->Pos, window->Pos + window->Size, false);
	surface_frame(window->DrawList, window->Pos + m, window->Pos + window->Size - m, rounding, fill ? *fill : current().background_contrast, level);
	window->DrawList->PopClipRect();
}

// glyph centred on its ink box
static void glyph_centered(ImDrawList * draw, const char * icon, const ImVec2 & center, float size, ImU32 col)
{
	ImGui::PushFont(nullptr, unscaled(size));
	unsigned int codepoint = 0;
	ImTextCharFromUtf8(&codepoint, icon, nullptr);
	const ImFontGlyph * glyph = ImGui::GetFontBaked()->FindGlyph(ImWchar(codepoint));
	if (glyph)
		draw->AddText({center.x - (glyph->X0 + glyph->X1) * 0.5f, center.y - (glyph->Y0 + glyph->Y1) * 0.5f}, col, icon);
	else
	{
		const ImVec2 ts = ImGui::CalcTextSize(icon);
		draw->AddText(center - ts * 0.5f, col, icon);
	}
	ImGui::PopFont();
}

void page_header(const std::string & title, const std::string & subtitle)
{
	const theme & t = current();

	ImGui::PushFont(nullptr, ImGui::GetStyle().FontSizeBase * metrics::font_title);
	ImGui::TextUnformatted(title.c_str());
	ImGui::PopFont();

	if (not subtitle.empty())
	{
		ImGui::PushStyleColor(ImGuiCol_Text, t.text_muted);
		ImGui::TextUnformatted(subtitle.c_str());
		ImGui::PopStyleColor();
	}
	ImGui::Dummy({0, ImGui::GetStyle().ItemSpacing.y});
}

// a rounded panel, not a child window, so the page stays one scroll surface, drawn behind
// the content using last frame's height, pair with end_card
namespace
{
struct card_frame
{
	ImGuiID id;
	ImVec2 origin;
	float width;
	float content_max_x_backup;
	float work_max_x_backup;
	ImVec2 padding;
};
std::vector<card_frame> card_stack;
std::unordered_map<ImGuiID, float> card_heights;

bool begin_card_impl(const char * id, ImVec2 padding)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	const theme & t = current();
	const ImGuiID gid = window->GetID(id);

	card_stack.push_back({gid,
	                      window->DC.CursorPos,
	                      ImGui::GetContentRegionAvail().x,
	                      window->ContentRegionRect.Max.x,
	                      window->WorkRect.Max.x,
	                      padding});
	const card_frame & cf = card_stack.back();

	if (const float h = card_heights[gid]; h > 0)
	{
		const ImVec2 bb_max = {cf.origin.x + cf.width, cf.origin.y + h};
		ImVec4 card = t.card;
		card.w *= background_alpha(); // card fill follows the panel opacity
		surface_frame(window->DrawList, cf.origin, bb_max, t.card_rounding, card, 1);
	}

	ImGui::PushID(id);
	window->ContentRegionRect.Max.x -= padding.x;
	window->WorkRect.Max.x -= padding.x;
	ImGui::Indent(padding.x);
	ImGui::SetCursorScreenPos({ImGui::GetCursorScreenPos().x, cf.origin.y + padding.y});
	ImGui::BeginGroup();
	return true;
}
} // namespace

bool begin_card(const char * id, const ImVec2 &)
{
	return begin_card_impl(id, metrics::card_padding);
}

bool begin_list_card(const char * id)
{
	return begin_card_impl(id, metrics::list_card_padding);
}

void end_card()
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	const card_frame cf = card_stack.back();
	card_stack.pop_back();

	ImGui::EndGroup();
	ImGui::Unindent(cf.padding.x);
	window->ContentRegionRect.Max.x = cf.content_max_x_backup;
	window->WorkRect.Max.x = cf.work_max_x_backup;
	ImGui::PopID();

	const float height = ImGui::GetItemRectMax().y + cf.padding.y - cf.origin.y;
	card_heights[cf.id] = height;

	// reserve the full card extent so layout and scrolling account for it
	ImGui::SetCursorScreenPos(cf.origin);
	ImGui::Dummy({cf.width, height});
}

void row_separator()
{
	const theme & t = current();
	ImVec4 edge = t.border;
	edge.w *= background_alpha(); // match the panel/card opacity
	if (card_stack.empty())
	{
		ImGui::PushStyleColor(ImGuiCol_Separator, edge);
		ImGui::Separator();
		ImGui::PopStyleColor();
		return;
	}

	// LVRR: a hairline across the whole box, like the rows of a boxed list
	const card_frame & cf = card_stack.back();
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	const float y = window->DC.CursorPos.y;
	ImGui::Dummy({0, t.border_size});
	window->DrawList->AddRectFilled({cf.origin.x + t.border_size, y}, {cf.origin.x + cf.width - t.border_size, y + t.border_size}, t.col(edge));
}

float setting_label(const std::string & title, const std::string & description, float control_width)
{
	const theme & t = current();
	const ImGuiStyle & style = ImGui::GetStyle();

	const ImVec2 start = ImGui::GetCursorPos();
	const float avail = ImGui::GetContentRegionAvail().x;
	float text_w = avail - control_width - style.ItemSpacing.x;
	if (text_w < 1)
		text_w = avail;

	ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, {style.ItemSpacing.x, metrics::label_line_gap});
	ImGui::PushTextWrapPos(start.x + text_w);
	ImGui::TextUnformatted(title.c_str());
	ImGui::PopTextWrapPos();
	if (not description.empty())
	{
		ImGui::PushFont(nullptr, style.FontSizeBase * metrics::font_description);
		ImGui::PushStyleColor(ImGuiCol_Text, t.text_muted);
		// never wider than half the row, wrap to as many lines as needed
		const float desc_w = ImMin(text_w, avail * 0.5f);
		ImGui::PushTextWrapPos(start.x + desc_w);
		ImGui::TextUnformatted(description.c_str());
		ImGui::PopTextWrapPos();
		ImGui::PopStyleColor();
		ImGui::PopFont();
	}
	ImGui::PopStyleVar();

	// right-aligned control slot, vertically centered against the label block
	const float label_h = ImGui::GetCursorPos().y - start.y;
	const float ctrl_h = ImGui::GetFrameHeight() * metrics::control_height;
	const float ctrl_y = start.y + ImMax(0.f, (label_h - ctrl_h) * 0.5f);
	ImGui::SetCursorPos({start.x + avail - control_width, ctrl_y});
	return start.y + label_h;
}

namespace
{
struct button_colors
{
	ImVec4 bg, bg_hovered, bg_active, text;
};

button_colors colors_for(button_style s, const theme & t)
{
	switch (s)
	{
		case button_style::primary:
			return {t.accent, t.accent_hovered, t.accent_active, t.on_accent};
		case button_style::secondary:
			return {t.control, t.control_hovered, t.control_active, t.text};
		case button_style::danger:
			return {t.danger_soft, mix(t.danger_soft, t.danger, 0.12f), mix(t.danger_soft, t.danger, 0.22f), t.danger};
		case button_style::ghost:
			return {{0, 0, 0, 0}, t.hover, t.pressed, t.text};
	}
	return {};
}

// LVRR: One buttons are flat fills
void button_frame(ImDrawList * draw, const ImRect & bb, const ImVec4 & bg, float rounding)
{
	if (bg.w <= 0)
		return;
	draw->AddRectFilled(bb.Min, bb.Max, current().col(bg), rounding);
}

// button labels are semibold
void push_label_font()
{
	ImGui::PushFont(semibold_font(), 0);
}
} // namespace

std::string icon_label(const char * icon, const std::string & label)
{
	return std::string(icon) + "  " + label;
}

float button_width(const std::string & label)
{
	push_label_font();
	const float w = ImGui::CalcTextSize(label.c_str(), nullptr, true).x + metrics::button_padding.x * 2;
	ImGui::PopFont();
	return w;
}

float button_width(const char * icon, const std::string & label)
{
	push_label_font();
	ImGui::PushFont(nullptr, unscaled(ImGui::GetFontSize() * metrics::button_label_glyph));
	const float icon_w = ImGui::CalcTextSize(icon).x;
	ImGui::PopFont();
	const float gap = ImGui::GetFontSize() * 0.5f;
	const float w = icon_w + gap + ImGui::CalcTextSize(label.c_str()).x + metrics::button_padding.x * 2;
	ImGui::PopFont();
	return w;
}

ImVec2 chip_size(const std::string & label)
{
	const ImVec2 ts = ImGui::CalcTextSize(label.c_str());
	return {ts.x + metrics::chip_padding.x * 2, ts.y + metrics::chip_padding.y * 2};
}

float chip_width(const std::string & label, bool dot, float height)
{
	// mirrors the footprint computed in chip()
	const ImVec2 ts = ImGui::CalcTextSize(label.c_str());
	const float pad_x = height > 0 ? metrics::chip_pill_padding_x : metrics::chip_padding.x;
	const float dot_r = ts.y * 0.26f;
	const float dot_gap = dot ? dot_r * 2 + pad_x * 0.55f : 0;
	return ts.x + pad_x * 2 + dot_gap;
}

bool button(const std::string & label, button_style s, const ImVec2 & size)
{
	const theme & t = current();

	// control height by default, width auto-fits the label
	const float h = size.y > 0 ? size.y : control_size();

	const button_colors c = colors_for(s, t);

	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;
	const ImGuiStyle & style = ImGui::GetStyle();
	push_label_font();
	const ImVec2 label_size = ImGui::CalcTextSize(label.c_str(), nullptr, true);
	const float w = size.x > 0 ? size.x : label_size.x + metrics::button_padding.x * 2;
	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + ImVec2{w, h});
	ImGui::ItemSize(bb, style.FramePadding.y);
	const ImGuiID id = window->GetID(label.c_str());
	if (not ImGui::ItemAdd(bb, id))
	{
		ImGui::PopFont();
		return false;
	}

	bool hovered, held;
	const bool pressed = ImGui::ButtonBehavior(bb, id, &hovered, &held);
	hover_haptic();

	ImVec4 bg = c.bg;
	if (held)
		bg = c.bg_active;
	else if (hovered)
		bg = c.bg_hovered;
	button_frame(window->DrawList, bb, bg, t.rounding);

	ImGui::PushStyleColor(ImGuiCol_Text, c.text);
	ImGui::RenderTextClipped(bb.Min + metrics::button_padding, bb.Max - metrics::button_padding, label.c_str(), nullptr, &label_size, {0.5f, 0.5f}, &bb);
	ImGui::PopStyleColor();
	ImGui::PopFont();
	return pressed;
}

bool button(const char * icon, const std::string & label, button_style s, const ImVec2 & size)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;

	ImGuiContext & g = *GImGui;
	const ImGuiStyle & style = g.Style;
	const theme & t = current();

	const float h = size.y > 0 ? size.y : control_size();
	push_label_font();
	const float glyph_size = unscaled(ImGui::GetFontSize() * metrics::button_label_glyph);
	const float gap = ImGui::GetFontSize() * 0.5f;

	ImGui::PushFont(nullptr, glyph_size);
	const float icon_w = ImGui::CalcTextSize(icon).x;
	ImGui::PopFont();
	const ImVec2 label_size = ImGui::CalcTextSize(label.c_str());
	const float content_w = icon_w + gap + label_size.x;
	const float w = size.x > 0 ? size.x : content_w + metrics::button_padding.x * 2;

	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + ImVec2{w, h});
	ImGui::ItemSize(bb, style.FramePadding.y);
	const ImGuiID id = window->GetID(label.c_str());
	if (not ImGui::ItemAdd(bb, id))
	{
		ImGui::PopFont();
		return false;
	}

	bool hovered, held;
	const bool pressed = ImGui::ButtonBehavior(bb, id, &hovered, &held);
	hover_haptic();

	const button_colors c = colors_for(s, t);
	ImVec4 bg = c.bg;
	if (held)
		bg = c.bg_active;
	else if (hovered)
		bg = c.bg_hovered;

	ImDrawList * draw = window->DrawList;
	button_frame(draw, bb, bg, t.rounding);

	const ImU32 col = t.col(c.text);
	const ImVec2 center = bb.GetCenter();
	float x = center.x - content_w * 0.5f;

	// icon centred vertically on its ink box, like icon_button
	ImGui::PushFont(nullptr, glyph_size);
	unsigned int codepoint = 0;
	ImTextCharFromUtf8(&codepoint, icon, nullptr);
	const ImFontGlyph * glyph = ImGui::GetFontBaked()->FindGlyph(ImWchar(codepoint));
	if (glyph)
		draw->AddText({x, center.y - (glyph->Y0 + glyph->Y1) * 0.5f}, col, icon);
	else
		draw->AddText({x, center.y - ImGui::GetFontSize() * 0.5f}, col, icon);
	ImGui::PopFont();

	x += icon_w + gap;
	draw->AddText({x, center.y - label_size.y * 0.5f}, col, label.c_str());
	ImGui::PopFont();

	return pressed;
}

bool icon_button(const char * icon, const ImVec2 & size_arg, bool active, const std::string & tooltip)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;

	ImGuiContext & g = *GImGui;
	const ImGuiStyle & style = g.Style;
	const theme & t = current();

	// square touch target, same height as the other controls
	const float side = control_size();
	const ImVec2 size = size_arg.x > 0 ? size_arg : ImVec2{side, side};

	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + size);
	ImGui::ItemSize(bb, style.FramePadding.y);
	const ImGuiID gid = window->GetID(icon);
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	bool hovered, held;
	const bool pressed = ImGui::ButtonBehavior(bb, gid, &hovered, &held);
	hover_haptic();
	if (hovered)
		show_tooltip(tooltip);

	ImVec4 bg = active ? t.accent : t.control;
	if (held)
		bg = active ? t.accent_active : t.control_active;
	else if (hovered)
		bg = active ? t.accent_hovered : t.control_hovered;

	ImDrawList * draw = window->DrawList;
	button_frame(draw, bb, bg, t.rounding);

	// centre the glyph on its ink box, not CalcTextSize
	glyph_centered(draw, icon, bb.GetCenter(), std::min(size.x, size.y) * metrics::icon_button_glyph, t.col(active ? t.on_accent : t.text));

	return pressed;
}

bool cancel_progress_button(const char * id, float fraction, const ImVec2 & size_arg, const std::string & tooltip)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;

	ImGuiContext & g = *GImGui;
	const theme & t = current();

	const float side = control_size();
	const ImVec2 size = size_arg.x > 0 ? size_arg : ImVec2{side, side};

	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + size);
	ImGui::ItemSize(bb, g.Style.FramePadding.y);
	const ImGuiID gid = window->GetID(id);
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	bool hovered, held;
	const bool pressed = ImGui::ButtonBehavior(bb, gid, &hovered, &held);
	hover_haptic();
	if (hovered)
		show_tooltip(tooltip);

	ImDrawList * draw = window->DrawList;
	const ImVec2 center = bb.GetCenter();
	const float thickness = ImMax(2.f, size.y * 0.09f);
	const float radius = ImMin(size.x, size.y) * 0.5f - thickness;

	// faint full ring as the track
	draw->PathArcTo(center, radius, 0, 2 * IM_PI, 48);
	draw->PathStroke(t.col(t.control_hovered), thickness);

	const ImU32 accent = t.col(t.accent);
	if (fraction >= 0)
	{
		// determinate arc, clockwise from the top
		const float a0 = -IM_PI * 0.5f;
		draw->PathArcTo(center, radius, a0, a0 + ImClamp(fraction, 0.f, 1.f) * 2 * IM_PI, 48);
		draw->PathStroke(accent, thickness);
	}
	else
	{
		// indeterminate: a rotating quarter arc
		const float a0 = float(g.Time) * 3.5f;
		draw->PathArcTo(center, radius, a0, a0 + IM_PI * 0.5f, 24);
		draw->PathStroke(accent, thickness);
	}

	// stop glyph in the centre, darkening on hover to read as a cancel target
	glyph_centered(draw, ICON_LVRR_STOP, center, size.y * metrics::icon_button_glyph * 0.7f, t.col(hovered or held ? t.text : t.text_muted));

	return pressed;
}

// Horizontal space a trailing reset slot consumes, gap included
float reset_slot_width()
{
	return control_size() + ImGui::GetStyle().ItemSpacing.x;
}

// trailing reset button on a value control's row, empty slot when not shown so the right edge stays put
static bool reset_slot(const char * id, bool show)
{
	const float side = control_size();
	ImGui::SameLine(0, ImGui::GetStyle().ItemSpacing.x);
	if (not show)
	{
		ImGui::Dummy({side, side});
		return false;
	}
	ImGui::PushID(id);
	const bool clicked = icon_button(ICON_LVRR_RESET, {side, side}, false, "Reset to default");
	ImGui::PopID();
	return clicked;
}

bool toggle(const char * id, bool * v, const bool * default_value)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;

	const theme & t = current();
	const ImGuiStyle & style = ImGui::GetStyle();

	// an Adwaita switch, centred in a box as tall as the other controls
	const float height = metrics::toggle_track;
	const float width = height * metrics::toggle_aspect;
	const float radius = height * 0.5f;

	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + ImVec2(width, control_size()));
	ImGui::ItemSize(bb, style.FramePadding.y);
	const ImGuiID gid = window->GetID(id);
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	bool hovered, held;
	bool changed = ImGui::ButtonBehavior(bb, gid, &hovered, &held);
	hover_haptic();
	if (changed)
		*v = not *v;

	ImVec4 track = *v ? t.accent : t.control_hovered;
	if (hovered)
		track = *v ? t.accent_hovered : t.control_active;

	ImDrawList * draw = window->DrawList;
	const ImVec2 min{bb.Min.x, bb.GetCenter().y - radius};
	draw->AddRectFilled(min, min + ImVec2{width, height}, t.col(track), radius);
	const ImVec2 knob{*v ? min.x + width - radius : min.x + radius, min.y + radius};
	const float r = radius - metrics::toggle_knob_inset;
	shadow(draw, knob - ImVec2{r, r}, knob + ImVec2{r, r}, r, 1);
	draw->AddCircleFilled(knob, r, t.col({1, 1, 1, 1}));

	if (default_value and reset_slot(id, *v != *default_value))
	{
		*v = *default_value;
		changed = true;
	}

	return changed;
}

bool slider_int(const char * id, int * v, int v_min, int v_max, const char * format, const ImVec2 & size_arg, const int * default_value)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;

	ImGuiContext & g = *GImGui;
	const ImGuiStyle & style = g.Style;
	const theme & t = current();
	const ImGuiID gid = window->GetID(id);

	ImVec2 size = ImGui::CalcItemSize(size_arg, ImGui::CalcItemWidth(), control_size());
	if (default_value)
		size.x -= reset_slot_width();
	const ImVec2 pos = window->DC.CursorPos;
	ImRect bb(pos, pos + size);
	ImGui::ItemSize(bb, style.FramePadding.y);
	if (not ImGui::ItemAdd(bb, gid, &bb))
		return false;

	// a thin track, the value at its right, as wide as the widest so the track keeps its length;
	// the thumb stays inside the track's ends
	char buf[64];
	float value_w = 0;
	for (int x: {v_min, v_max})
	{
		std::snprintf(buf, sizeof(buf), format, x);
		value_w = std::max(value_w, ImGui::CalcTextSize(buf).x);
	}
	std::snprintf(buf, sizeof(buf), format, *v);
	const float thumb = metrics::slider_thumb;
	const ImRect track_bb({bb.Min.x + thumb * 0.5f, bb.Min.y}, {bb.Max.x - value_w - ImGui::GetStyle().ItemSpacing.x - thumb * 0.5f, bb.Max.y});

	// activation, mirrors ImGui::SliderScalar
	const bool hovered = ImGui::ItemHoverable(bb, gid, g.LastItemData.ItemFlags);
	hover_haptic();
	const bool clicked = hovered and ImGui::IsMouseClicked(ImGuiMouseButton_Left, ImGuiInputFlags_None, gid);
	if (clicked or g.NavActivateId == gid)
	{
		if (clicked)
			ImGui::SetKeyOwner(ImGuiKey_MouseLeft, gid);
		ImGui::SetActiveID(gid, window);
		ImGui::SetFocusID(gid, window);
		ImGui::FocusWindow(window);
		g.ActiveIdUsingNavDirMask |= (1 << ImGuiDir_Left) | (1 << ImGuiDir_Right);
	}

	ImRect grab;
	const bool changed = ImGui::SliderBehavior(track_bb, gid, ImGuiDataType_S32, v, &v_min, &v_max, format, ImGuiSliderFlags_None, &grab);
	if (changed)
	{
		ImGui::MarkItemEdited(gid);
		std::snprintf(buf, sizeof(buf), format, *v);
	}

	const float fraction = v_max > v_min ? float(*v - v_min) / float(v_max - v_min) : 0;
	const float fill_x = track_bb.Min.x + track_bb.GetWidth() * fraction;
	const float cy = bb.GetCenter().y;
	const float th = metrics::slider_track * 0.5f;

	ImDrawList * draw = window->DrawList;
	draw->AddRectFilled({track_bb.Min.x, cy - th}, {track_bb.Max.x, cy + th}, t.col(t.control_hovered), th);
	if (fraction > 0)
		draw->AddRectFilled({track_bb.Min.x, cy - th}, {fill_x, cy + th}, t.col(t.accent), th);
	// white thumb with a hairline
	const ImVec2 c{fill_x, cy};
	const float r = thumb * 0.5f;
	shadow(draw, c - ImVec2{r, r}, c + ImVec2{r, r}, r, 1);
	draw->AddCircleFilled(c, r, t.col({1, 1, 1, 1}));
	draw->AddCircle(c, r, t.col(t.border), 0, t.border_size);

	const ImVec2 ts = ImGui::CalcTextSize(buf);
	draw->AddText({bb.Max.x - ts.x, cy - ts.y * 0.5f}, t.col(t.text_muted), buf);

	bool reset = false;
	if (default_value and reset_slot(id, *v != *default_value))
	{
		*v = *default_value;
		reset = true;
	}

	return changed or reset;
}

bool segmented(const char * id, const std::vector<std::string> & options, int * selected, const ImVec2 & size_arg, const int * default_value)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems or options.empty())
		return false;

	ImGuiContext & g = *GImGui;
	const ImGuiStyle & style = g.Style;
	const theme & t = current();

	ImVec2 size = ImGui::CalcItemSize(size_arg, ImGui::CalcItemWidth(), control_size());
	if (default_value)
		size.x -= reset_slot_width();
	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + size);
	ImGui::ItemSize(bb, style.FramePadding.y);
	const ImGuiID gid = window->GetID(id);
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	const bool hovered = ImGui::ItemHoverable(bb, gid, g.LastItemData.ItemFlags);
	hover_haptic();
	// claim the active id on press so a drag here scrolls nothing (see ScrollWhenDragging)
	if (hovered and ImGui::IsMouseClicked(ImGuiMouseButton_Left))
	{
		ImGui::SetActiveID(gid, window);
		ImGui::FocusWindow(window);
	}
	if (g.ActiveId == gid and not g.IO.MouseDown[ImGuiMouseButton_Left])
		ImGui::ClearActiveID();

	// a One view switcher: a filled pill, the selected view raised on the surface color
	ImDrawList * draw = window->DrawList;
	draw->AddRectFilled(bb.Min, bb.Max, t.col(t.control), t.rounding);

	const int n = int(options.size());
	const float pad = metrics::segmented_inset;
	const float seg_w = (size.x - 2 * pad) / n;
	const float inner = std::max(0.f, t.rounding - pad);
	bool changed = false;

	for (int i = 0; i < n; ++i)
	{
		const ImRect seg({bb.Min.x + pad + seg_w * i, bb.Min.y + pad}, {bb.Min.x + pad + seg_w * (i + 1), bb.Max.y - pad});
		const bool seg_hovered = hovered and seg.Contains(g.IO.MousePos);
		if (seg_hovered and ImGui::IsMouseClicked(ImGuiMouseButton_Left))
		{
			*selected = i;
			changed = true;
		}

		const bool active = *selected == i;
		if (active)
			surface_frame(draw, seg.Min, seg.Max, inner, t.card, 1);
		else if (seg_hovered)
			draw->AddRectFilled(seg.Min, seg.Max, t.col(t.hover), inner);

		const char * label = options[i].c_str();
		if (active)
			push_label_font();
		const ImVec2 ts = ImGui::CalcTextSize(label);
		draw->AddText({seg.Min.x + (seg_w - ts.x) * 0.5f, bb.Min.y + (size.y - ts.y) * 0.5f}, t.col(active ? t.text : t.text_muted), label);
		if (active)
			ImGui::PopFont();
	}

	if (default_value and reset_slot(id, *selected != *default_value))
	{
		*selected = *default_value;
		changed = true;
	}

	return changed;
}

namespace
{
// where combo modals open, in imgui display coords, set each frame by the lobby
ImVec2 popup_center = {0, 0};
// height of the popup layer, caps tall combo lists so they scroll, 0 = unbounded
float popup_available_height = 0;

// one selectable option inside the combo modal, tall full-width touch target with an optional description
bool combo_row(const combo_item & item, bool selected, bool checked)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	const theme & t = current();

	const float row_h = metrics::combo_row_height;
	const float row_w = ImGui::GetContentRegionAvail().x;
	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + ImVec2(row_w, row_h));
	ImGui::ItemSize(bb, 0);
	const ImGuiID gid = window->GetID(item.name);
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	bool hovered, held;
	const bool pressed = ImGui::ButtonBehavior(bb, gid, &hovered, &held);
	hover_haptic();

	// menu rows: selection in the container color, concentric with the popover
	ImDrawList * draw = window->DrawList;
	const float r = std::max(0.f, t.card_rounding - metrics::list_card_padding.x);
	if (selected)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.container), r);
	if (held)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.pressed), r);
	else if (hovered)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.hover), r);

	if (checked)
		glyph_centered(draw, ICON_LVRR_CHECK, {bb.Max.x - metrics::combo_padding.x - metrics::shell_nav_glyph * 0.5f, bb.GetCenter().y}, metrics::shell_nav_glyph, t.col(t.accent));

	const float tx = bb.Min.x + metrics::combo_padding.x;
	const ImU32 name_col = t.col(selected ? t.on_container : t.text);
	const ImVec2 nts = ImGui::CalcTextSize(item.name);

	if (item.description and item.description[0])
	{
		const float desc_size = ImGui::GetStyle().FontSizeBase * metrics::font_description;
		ImGui::PushFont(nullptr, desc_size);
		const ImVec2 dts = ImGui::CalcTextSize(item.description);
		const float total = nts.y + metrics::label_line_gap + dts.y;
		const float y = bb.Min.y + (row_h - total) * 0.5f;
		draw->AddText({tx, y + nts.y + metrics::label_line_gap}, t.col(selected ? t.on_container : t.text_muted), item.description);
		ImGui::PopFont();
		draw->AddText({tx, y}, name_col, item.name);
	}
	else
	{
		draw->AddText({tx, bb.Min.y + (row_h - nts.y) * 0.5f}, name_col, item.name);
	}

	return pressed;
}

bool combo_impl(const char * id, const std::string & title, const char * preview, const std::vector<combo_item> & items, int * selected, std::vector<char> * selected_multi, float width, const int * default_value, const std::vector<char> * default_value_multi, bool close_on_click)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems or items.empty())
		return false;

	const ImGuiStyle & style = ImGui::GetStyle();
	const theme & t = current();
	const ImGuiID gid = window->GetID(id);

	// closed box, same height as the other controls so setting rows line up
	ImVec2 size = ImGui::CalcItemSize({width, 0}, ImGui::CalcItemWidth(), control_size());
	if (default_value or default_value_multi)
		size.x -= reset_slot_width();
	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + size);
	ImGui::ItemSize(bb, style.FramePadding.y);
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	bool hovered, held;
	const bool clicked = ImGui::ButtonBehavior(bb, gid, &hovered, &held);
	hover_haptic();

	ImVec4 box = t.control;
	if (held)
		box = t.control_active;
	else if (hovered)
		box = t.control_hovered;

	ImDrawList * draw = window->DrawList;
	draw->AddRectFilled(bb.Min, bb.Max, t.col(box), t.rounding);

	const int sel = selected ? ImClamp(*selected, 0, int(items.size()) - 1) : -1;

	// preview, left aligned and clipped to the box
	const ImVec2 ts = ImGui::CalcTextSize(preview);
	const float pad_x = metrics::combo_padding.x;
	draw->PushClipRect(bb.Min, bb.Max, true);
	draw->AddText({bb.Min.x + pad_x, bb.Min.y + (size.y - ts.y) * 0.5f}, t.col(t.text), preview);
	draw->PopClipRect();

	// chevron on the right
	glyph_centered(draw, ICON_LVRR_PAN_DOWN, {bb.Max.x - pad_x - metrics::combo_chevron * 0.5f, bb.GetCenter().y}, metrics::combo_chevron * 2, t.col(t.text_muted));

	char popup_id[64];
	std::snprintf(popup_id, sizeof(popup_id), "##combo_%08X", gid);
	if (clicked)
		ImGui::OpenPopup(popup_id);

	ImGui::SetNextWindowPos(popup_center, ImGuiCond_Always, {0.5f, 0.5f});
	ImGui::SetNextWindowSize({ImMax(size.x, metrics::combo_modal_min_width) + 2 * metrics::popup_margin, 0});

	// a popover: float color, hairline, shadow
	push_float_style(metrics::list_card_padding);

	bool changed = false;
	if (ImGui::BeginPopupModal(popup_id, nullptr, ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoMove | ImGuiWindowFlags_AlwaysAutoResize))
	{
		float_frame(t.card_rounding, 2);
		if (not title.empty())
		{
			// section label, aligned with the rows' text
			ImGui::Indent(metrics::combo_padding.x);
			ImGui::Dummy({0, metrics::list_card_padding.y});
			ImGui::PushFont(semibold_font(), style.FontSizeBase * metrics::font_description);
			ImGui::PushStyleColor(ImGuiCol_Text, t.text_muted);
			ImGui::TextUnformatted(title.c_str());
			ImGui::PopStyleColor();
			ImGui::PopFont();
			ImGui::Unindent(metrics::combo_padding.x);
		}

		// cap the list height so a long list scrolls inside the modal instead of clipping
		const float row_h = metrics::combo_row_height;
		// include the inter-row gaps or a scrollbar shows even when it all fits
		float list_h = int(items.size()) * row_h + ImMax(0, int(items.size()) - 1) * style.ItemSpacing.y;
		if (popup_available_height > 0)
		{
			const float title_h = title.empty() ? 0 : ImGui::GetTextLineHeight() + style.ItemSpacing.y + metrics::list_card_padding.y;
			const float max_list = popup_available_height * 0.9f - 2 * (metrics::list_card_padding.y + metrics::popup_margin) - title_h;
			list_h = ImMin(list_h, ImMax(max_list, row_h));
		}

		ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, {0, 0});
		ImGui::BeginChild("##list", {0, list_h});
		for (int i = 0; i < int(items.size()); ++i)
		{
			const bool is_selected = i == sel;
			const bool is_selected_multi = selected_multi and i < selected_multi->size() and (*selected_multi)[i];
			if (combo_row(items[i], is_selected, is_selected_multi))
			{
				if (selected)
					*selected = i;

				if (selected_multi)
				{
					if (selected_multi->size() <= i)
						selected_multi->resize(i + 1);
					(*selected_multi)[i] = not(*selected_multi)[i];
				}

				changed = true;
				if (close_on_click)
					ImGui::CloseCurrentPopup();
			}
			if (is_selected and ImGui::IsWindowAppearing())
				ImGui::SetScrollHereY(0.5f); // reveal the current value when opening
		}
		ImGui::EndChild();
		ImGui::PopStyleVar();

		// click outside dismisses, AllowWhenBlockedByActiveItem so pressing a row still counts as inside
		if (ImGui::IsMouseClicked(ImGuiMouseButton_Left) and not ImGui::IsWindowHovered(ImGuiHoveredFlags_RootAndChildWindows | ImGuiHoveredFlags_AllowWhenBlockedByActiveItem))
			ImGui::CloseCurrentPopup();

		ImGui::EndPopup();
	}

	pop_float_style();

	bool show_reset_slot = (selected and default_value and *selected != *default_value) or
	                       (selected_multi and default_value_multi and *selected_multi != *default_value_multi);

	if (reset_slot(id, show_reset_slot))
	{
		if (selected and default_value)
			*selected = *default_value;

		if (selected_multi and default_value_multi)
			*selected_multi = *default_value_multi;

		changed = true;
	}

	return changed;
}
} // namespace

void set_popup_center(const ImVec2 & center, float available_height)
{
	popup_center = center;
	popup_available_height = available_height;
}

bool combo(const char * id, const std::string & title, const std::vector<combo_item> & items, int * selected, float width, const int * default_value)
{
	const int sel = ImClamp(*selected, 0, int(items.size()) - 1);
	const char * preview = items[sel].name;

	return combo_impl(id, title, preview, items, selected, nullptr, width, default_value, nullptr, true);
}

bool combo_multi(const char * id, const std::string & title, const std::string & preview, const std::vector<combo_item> & items, std::vector<char> * selected, float width, const std::vector<char> * default_value)
{
	return combo_impl(id, title, preview.c_str(), items, nullptr, selected, width, nullptr, default_value, false);
}

void chip(const std::string & label, chip_style style, bool dot, float height)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return;

	const theme & t = current();

	// foreground (text/dot/icon) and the pill fill
	ImVec4 fg = t.text;
	ImVec4 bg = t.control;
	switch (style)
	{
		case chip_style::neutral:
			break;
		case chip_style::muted:
			fg = t.text_muted;
			break;
		case chip_style::accent:
			fg = t.on_container;
			bg = t.container;
			break;
		case chip_style::success:
			fg = t.success;
			bg = t.success_soft;
			break;
		case chip_style::warning:
			fg = t.warning;
			bg = t.warning_soft;
			break;
		case chip_style::danger:
			fg = t.danger;
			bg = t.danger_soft;
			break;
	}

	const ImVec2 ts = ImGui::CalcTextSize(label.c_str());
	const ImVec2 pad = metrics::chip_padding;
	// tall pill chips (height override) get roomier sides
	const float pad_x = height > 0 ? metrics::chip_pill_padding_x : pad.x;
	const float dot_r = ts.y * 0.26f;
	const float dot_gap = dot ? dot_r * 2 + pad_x * 0.55f : 0;
	const ImVec2 size = {ts.x + pad_x * 2 + dot_gap, height > 0 ? height : ts.y + pad.y * 2};

	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + size);
	ImGui::ItemSize(bb);
	if (not ImGui::ItemAdd(bb, 0))
		return;

	ImDrawList * draw = window->DrawList;
	draw->AddRectFilled(bb.Min, bb.Max, t.col(bg), size.y * 0.5f);

	float tx = bb.Min.x + pad_x;
	if (dot)
	{
		draw->AddCircleFilled({tx + dot_r, bb.Min.y + size.y * 0.5f}, dot_r, t.col(fg));
		tx += dot_r * 2 + pad_x * 0.55f;
	}
	draw->AddText({tx, bb.Min.y + (size.y - ts.y) * 0.5f}, t.col(fg), label.c_str());
}

void nav_section(const std::string & label)
{
	const theme & t = current();
	const ImGuiStyle & style = ImGui::GetStyle();

	ImGui::Dummy({0, metrics::nav_section_gap}); // gap above the section
	ImGui::Indent(metrics::combo_padding.x);     // align with the nav_item icons
	ImGui::PushFont(semibold_font(), style.FontSizeBase * metrics::font_description);
	ImGui::PushStyleColor(ImGuiCol_Text, t.text_muted);
	ImGui::TextUnformatted(label.c_str());
	ImGui::PopStyleColor();
	ImGui::PopFont();
	ImGui::Unindent(metrics::combo_padding.x);
	ImGui::Dummy({0, 4});
}

bool nav_item(const char * icon, const std::string & label, bool selected)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	if (window->SkipItems)
		return false;

	const theme & t = current();
	const ImGuiStyle & style = ImGui::GetStyle();

	const float h = metrics::shell_nav_row;
	const float w = ImGui::GetContentRegionAvail().x;
	const ImVec2 pos = window->DC.CursorPos;
	const ImRect bb(pos, pos + ImVec2(w, h));
	ImGui::ItemSize(bb, style.FramePadding.y);
	const ImGuiID gid = window->GetID(label.c_str());
	if (not ImGui::ItemAdd(bb, gid))
		return false;

	bool hovered, held;
	const bool pressed = ImGui::ButtonBehavior(bb, gid, &hovered, &held);
	hover_haptic();

	// selection in the container color, hover as a light overlay
	ImDrawList * draw = window->DrawList;
	if (selected)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.container), t.rounding);
	if (held)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.pressed), t.rounding);
	else if (hovered and not selected)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.hover), t.rounding);

	const float pad = metrics::combo_padding.x;
	const float glyph = metrics::shell_nav_glyph;
	const ImU32 fg = t.col(selected ? t.on_container : t.text);
	glyph_centered(draw, icon, {bb.Min.x + pad + glyph * 0.5f, bb.GetCenter().y}, glyph, fg);

	if (selected)
		push_label_font();
	const ImVec2 ls = ImGui::CalcTextSize(label.c_str());
	draw->AddText({bb.Min.x + pad + glyph + pad, bb.Min.y + (h - ls.y) * 0.5f}, fg, label.c_str());
	if (selected)
		ImGui::PopFont();

	return pressed;
}

static int input_text_resize(ImGuiInputTextCallbackData * data)
{
	if (data->EventFlag == ImGuiInputTextFlags_CallbackResize)
	{
		std::string & s = *static_cast<std::string *>(data->UserData);
		s.resize(data->BufTextLen);
		data->Buf = s.data();
	}
	return 0;
}

// common frame styling for the input widgets, paired with pop_input_style: filled entries
static void push_input_style(float height)
{
	const theme & t = current();
	ImGui::PushStyleColor(ImGuiCol_FrameBg, t.control);
	ImGui::PushStyleColor(ImGuiCol_FrameBgHovered, t.control_hovered);
	ImGui::PushStyleColor(ImGuiCol_FrameBgActive, t.control);
	ImGui::PushStyleColor(ImGuiCol_Text, t.text);
	ImGui::PushStyleColor(ImGuiCol_TextDisabled, t.text_faint);
	ImGui::PushStyleColor(ImGuiCol_Border, t.border);
	ImGui::PushStyleVar(ImGuiStyleVar_FrameRounding, t.rounding);
	ImGui::PushStyleVar(ImGuiStyleVar_FrameBorderSize, 0);
	ImGui::PushStyleVar(ImGuiStyleVar_FramePadding, {metrics::input_padding_x, (height - ImGui::GetFontSize()) * 0.5f});
}

// focused entries get an accent border
static void pop_input_style()
{
	ImGui::PopStyleVar(3);
	ImGui::PopStyleColor(6);
	if (ImGui::IsItemActive())
	{
		const theme & t = current();
		ImGui::GetWindowDrawList()->AddRect(ImGui::GetItemRectMin(), ImGui::GetItemRectMax(), t.col(t.accent), t.rounding, 0, metrics::focus_ring);
	}
}

bool input_text(const char * id, std::string & text, const char * hint, float width)
{
	push_input_style(control_size());
	const bool changed = ImGui::InputTextEx(id, hint, text.data(), int(text.size()) + 1, {width, 0}, ImGuiInputTextFlags_CallbackResize, input_text_resize, &text);
	pop_input_style();
	hover_haptic();
	return changed;
}

bool input_int(const char * id, int * v, int step, int v_min, int v_max, float width)
{
	const ImGuiStyle & style = ImGui::GetStyle();
	const float side = control_size();
	const float gap = style.ItemSpacing.x;
	const float total = width > 0 ? width : ImGui::CalcItemWidth();
	const float field_w = total - 2 * (side + gap);

	ImGui::PushID(id);

	push_input_style(side);
	ImGui::SetNextItemWidth(field_w);
	bool changed = ImGui::InputInt("##field", v, 0, 0, ImGuiInputTextFlags_CharsDecimal); // step 0 hides native buttons
	pop_input_style();
	hover_haptic();

	ImGui::SameLine(0, gap);
	if (icon_button(ICON_LVRR_REMOVE, {side, side}))
	{
		*v -= step;
		changed = true;
	}
	ImGui::SameLine(0, gap);
	if (icon_button(ICON_LVRR_ADD, {side, side}))
	{
		*v += step;
		changed = true;
	}

	if (v_min != v_max)
		*v = ImClamp(*v, v_min, v_max);

	ImGui::PopID();
	return changed;
}

bool begin_modal(const char * id, const std::string & title, float width)
{
	const theme & t = current();
	const ImGuiStyle & style = ImGui::GetStyle();

	ImGui::SetNextWindowPos(popup_center, ImGuiCond_Always, {0.5f, 0.5f});
	if (width > 0)
		ImGui::SetNextWindowSize({width + 2 * metrics::popup_margin, 0});

	// a One dialog: surface color, window radius, hairline and shadow
	const ImVec2 padding{24 * metrics::one, 20 * metrics::one};
	push_float_style(padding);

	ImGuiWindowFlags flags = ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoMove;
	if (width <= 0)
		flags |= ImGuiWindowFlags_AlwaysAutoResize;

	const bool open = ImGui::BeginPopupModal(id, nullptr, flags);
	if (open)
	{
		float_frame(t.window_rounding, 2, &t.card);
		if (not title.empty())
		{
			ImGui::PushFont(bold_font(), style.FontSizeBase * metrics::font_modal_title);
			ImGui::TextUnformatted(title.c_str());
			ImGui::PopFont();
			ImGui::Dummy({0, style.ItemSpacing.y});
		}
	}
	else
		pop_float_style();
	return open;
}

void end_modal()
{
	ImGui::EndPopup();
	pop_float_style();
}

int confirm_modal(const char * id, const std::string & title, const std::string & message, const std::string & confirm_label, const std::string & cancel_label, bool danger)
{
	int result = 0;
	if (begin_modal(id, title, 520))
	{
		const theme & t = current();
		ImGui::PushStyleColor(ImGuiCol_Text, t.text_muted);
		ImGui::TextWrapped("%s", message.c_str());
		ImGui::PopStyleColor();
		ImGui::Dummy({0, 12});

		const float gap = ImGui::GetStyle().ItemSpacing.x;
		const float cancel_w = ImGui::CalcTextSize(cancel_label.c_str()).x + metrics::button_padding.x * 2;
		const float confirm_w = ImGui::CalcTextSize(confirm_label.c_str()).x + metrics::button_padding.x * 2;
		ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - cancel_w - confirm_w - gap);
		if (button(cancel_label, button_style::secondary, {cancel_w, 0}))
		{
			result = -1;
			ImGui::CloseCurrentPopup();
		}
		ImGui::SameLine();
		if (button(confirm_label, danger ? button_style::danger : button_style::primary, {confirm_w, 0}))
		{
			result = 1;
			ImGui::CloseCurrentPopup();
		}
		end_modal();
	}
	return result;
}

namespace
{
std::vector<ImVec2> list_row_after; // cursor to restore at end_list_row
}

list_row_result begin_list_row(const char * id, const char * icon, ImTextureID image, const std::string & title, const std::string & subtitle, bool selected, float trailing_width, float height, bool large_thumb, bool interactive)
{
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	const theme & t = current();
	const ImGuiStyle & style = ImGui::GetStyle();

	// measure title + (possibly multi-line) subtitle so the row can auto-size
	const float pad = metrics::list_row_pad;
	const float box = large_thumb ? metrics::list_row_box_large : metrics::list_row_box;
	const ImVec2 ts = ImGui::CalcTextSize(title.c_str());
	ImVec2 sub_sz{0, 0};
	if (not subtitle.empty())
	{
		ImGui::PushFont(nullptr, style.FontSizeBase * metrics::font_description);
		sub_sz = ImGui::CalcTextSize(subtitle.c_str());
		ImGui::PopFont();
	}
	const float text_h = ts.y + (subtitle.empty() ? 0 : metrics::label_line_gap + sub_sz.y);
	const float row_h = height > 0 ? height : ImMax(metrics::list_row_height, ImMax(box, text_h) + 2 * metrics::label_bottom_pad * metrics::one);
	const ImVec2 p0 = window->DC.CursorPos;
	const float avail = ImGui::GetContentRegionAvail().x;

	// non-interactive rows just reserve layout space, no click area and no hover highlight
	bool clicked = false;
	if (interactive)
	{
		// the click area excludes the trailing controls so they receive their own clicks
		ImGui::SetNextItemAllowOverlap();
		clicked = ImGui::InvisibleButton(id, {ImMax(avail - trailing_width, 1.f), row_h});
		hover_haptic();
	}
	else
		ImGui::Dummy({avail, row_h});
	const bool hovered = interactive and ImGui::IsItemHovered();
	list_row_after.push_back(window->DC.CursorPos);

	ImDrawList * draw = window->DrawList;
	const ImRect bb(p0, p0 + ImVec2(avail, row_h));
	// no tinted background for the selected row, callers mark the active item with a chip; the
	// hover is concentric with the boxed list around it
	if (hovered)
		draw->AddRectFilled(bb.Min, bb.Max, t.col(t.surface_alt), std::max(0.f, t.card_rounding - metrics::list_card_padding.x));

	// leading thumbnail or icon box, large_thumb spans the full row height
	const float thumb = large_thumb ? row_h - 2 * metrics::list_card_padding.y : box;
	const ImVec2 box_min = {p0.x + pad - metrics::list_card_padding.x, p0.y + (row_h - thumb) * 0.5f};
	const ImVec2 box_max = box_min + ImVec2(thumb, thumb);
	if (image)
		draw->AddImageRounded(image, box_min, box_max, {0, 0}, {1, 1}, IM_COL32_WHITE, t.small_rounding);
	else
	{
		draw->AddRectFilled(box_min, box_max, t.col(selected ? t.container : t.control), t.small_rounding + 2 * metrics::one);
		if (icon and icon[0])
			glyph_centered(draw, icon, (box_min + box_max) * 0.5f, metrics::shell_nav_glyph, t.col(selected ? t.on_container : t.text));
	}

	// title + subtitle
	const float tx = box_max.x + metrics::list_row_pad * 0.85f;
	const float total = subtitle.empty() ? ts.y : ts.y + metrics::label_line_gap + sub_sz.y;
	const float ty = p0.y + (row_h - total) * 0.5f;
	draw->AddText({tx, ty}, t.col(t.text), title.c_str());
	if (not subtitle.empty())
	{
		ImGui::PushFont(nullptr, style.FontSizeBase * metrics::font_description);
		draw->AddText({tx, ty + ts.y + metrics::label_line_gap}, t.col(t.text_muted), subtitle.c_str());
		ImGui::PopFont();
	}

	// trailing controls right-align against the row's padding
	return {bb.Min, {bb.Max.x - (pad - metrics::list_card_padding.x), bb.Max.y}, clicked};
}

void end_list_row()
{
	ImGui::GetCurrentWindow()->DC.CursorPos = list_row_after.back();
	list_row_after.pop_back();
}

int action_menu(const char * id, const char * icon, const std::vector<action_item> & items)
{
	const theme & t = current();

	// a flat overflow button, like a headerbar's
	{
		ImGuiWindow * window = ImGui::GetCurrentWindow();
		const float side = control_size();
		const ImRect bb(window->DC.CursorPos, window->DC.CursorPos + ImVec2{side, side});
		ImGui::ItemSize(bb);
		ImGui::PushID(id);
		const ImGuiID gid = window->GetID("##trigger");
		ImGui::PopID();
		if (ImGui::ItemAdd(bb, gid))
		{
			bool hovered, held;
			if (ImGui::ButtonBehavior(bb, gid, &hovered, &held))
				ImGui::OpenPopup(id);
			hover_haptic();
			if (held or hovered)
				window->DrawList->AddRectFilled(bb.Min, bb.Max, t.col(held ? t.control_hovered : t.control), t.rounding);
			glyph_centered(window->DrawList, icon, bb.GetCenter(), side * metrics::icon_button_glyph, t.col(t.text));
		}
	}

	int chosen = -1;

	const float pad = metrics::combo_padding.x;
	const float icon_w = metrics::shell_nav_glyph + pad;
	float content_w = 0;
	for (const auto & it: items)
		content_w = ImMax(content_w, icon_w + pad + ImGui::CalcTextSize(it.label).x + pad * 2 + ImGui::GetFontSize());

	// centred menu on the popup layer, like combo()
	ImGui::SetNextWindowPos(popup_center, ImGuiCond_Always, {0.5f, 0.5f});
	ImGui::SetNextWindowSize({ImMax(content_w, metrics::combo_modal_min_width) + 2 * metrics::popup_margin, 0});

	push_float_style(metrics::list_card_padding);

	if (ImGui::BeginPopupModal(id, nullptr, ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoMove | ImGuiWindowFlags_AlwaysAutoResize))
	{
		float_frame(t.card_rounding, 2);
		ImGuiWindow * window = ImGui::GetCurrentWindow();
		const float row_h = control_size();
		const float r = std::max(0.f, t.card_rounding - metrics::list_card_padding.x);
		ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, {0, 0});

		for (int i = 0; i < int(items.size()); ++i)
		{
			const action_item & it = items[i];
			ImGui::PushID(i);
			const float row_w = ImGui::GetContentRegionAvail().x;
			const ImVec2 pos = window->DC.CursorPos;
			const ImRect bb(pos, pos + ImVec2(row_w, row_h));
			ImGui::ItemSize(bb, 0);
			const ImGuiID gid = window->GetID("##row");
			if (ImGui::ItemAdd(bb, gid))
			{
				bool hovered, held;
				if (ImGui::ButtonBehavior(bb, gid, &hovered, &held))
				{
					chosen = i;
					ImGui::CloseCurrentPopup();
				}
				hover_haptic();

				// menu items: the hovered one in the container color, concentric with the menu
				ImDrawList * draw = window->DrawList;
				if (held or hovered)
					draw->AddRectFilled(bb.Min, bb.Max, t.col(it.danger ? t.danger_soft : t.container), r);

				const ImU32 fg = t.col(it.danger ? t.danger : (held or hovered) ? t.on_container
				                                                                : t.text);
				const float glyph = metrics::shell_nav_glyph;
				glyph_centered(draw, it.icon, {bb.Min.x + pad + glyph * 0.5f, bb.GetCenter().y}, glyph, fg);
				const ImVec2 ls = ImGui::CalcTextSize(it.label);
				draw->AddText({bb.Min.x + pad + icon_w, bb.Min.y + (row_h - ls.y) * 0.5f}, fg, it.label);
				if (it.checked)
					glyph_centered(draw, ICON_LVRR_CHECK, {bb.Max.x - pad - glyph * 0.5f, bb.GetCenter().y}, glyph, t.col(t.accent));
			}
			ImGui::PopID();
		}
		ImGui::PopStyleVar();

		// click outside dismisses, AllowWhenBlockedByActiveItem so pressing a row still counts as inside
		if (ImGui::IsMouseClicked(ImGuiMouseButton_Left) and not ImGui::IsWindowHovered(ImGuiHoveredFlags_RootAndChildWindows | ImGuiHoveredFlags_AllowWhenBlockedByActiveItem))
			ImGui::CloseCurrentPopup();

		ImGui::EndPopup();
	}

	pop_float_style();
	return chosen;
}

void top_bar(float height, ImTextureID logo, const std::vector<top_bar_item> & right_items)
{
	const float side = ImGui::GetFrameHeight() * metrics::control_height;
	const float gap = 8;
	const float margin = 24;

	// transparent child bg so the alpha-blended window background shows through
	ImGui::SetCursorPos({0, 0});
	ImGui::PushStyleColor(ImGuiCol_ChildBg, ImVec4{0, 0, 0, 0});
	ImGui::BeginChild("TopBar", {ImGui::GetWindowSize().x, height}, 0, ImGuiWindowFlags_NoScrollbar);
	{
		// logo: WiVRn mascot followed by the wordmark
		const float logo_size = 44;
		ImGui::SetCursorPos({margin, (height - logo_size) * 0.5f});
		if (logo)
			ImGui::Image(logo, {logo_size, logo_size});
		ImGui::SameLine(0, 12);
		ImGui::PushFont(nullptr, ImGui::GetStyle().FontSizeBase * 1.3f);
		ImGui::SetCursorPosY((height - ImGui::GetFontSize()) * 0.5f);
		ImGui::TextUnformatted("LVRR");
		ImGui::PopFont();

		// right-aligned cluster
		float cluster_w = 0;
		for (const auto & it: right_items)
			cluster_w += it.width;
		if (not right_items.empty())
			cluster_w += gap * (right_items.size() - 1);

		ImGui::SetCursorPos({ImGui::GetWindowSize().x - margin - cluster_w, (height - side) * 0.5f});
		for (size_t i = 0; i < right_items.size(); ++i)
		{
			ImGui::SetCursorPosY((height - side) * 0.5f);
			right_items[i].draw();
			if (i + 1 < right_items.size())
				ImGui::SameLine(0, gap);
		}
	}
	ImGui::EndChild();
	ImGui::PopStyleColor();
}

void begin_sidebar(float top_bar_h, float tab_width, int footer_items)
{
	// shares the transparent window background, sections separated by shell_dividers
	ImGui::PushStyleColor(ImGuiCol_ChildBg, ImVec4{0, 0, 0, 0});
	ImGui::SetCursorPos({0, top_bar_h});
	ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, {16, 16});
	ImGui::BeginChild("Tabs", {tab_width, ImGui::GetWindowSize().y - top_bar_h}, ImGuiChildFlags_AlwaysUseWindowPadding);
	ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, {0, 4});

	// nav list scrolls in a child that fills the rest, footer stays pinned in footer_items rows
	const float row_h = ImGui::GetFrameHeight() * metrics::control_height + ImGui::GetStyle().ItemSpacing.y;
	sidebar_footer_height = footer_items > 0 ? footer_items * row_h : 0;
	ImGui::BeginChild("TabsScroll", {0, ImGui::GetContentRegionAvail().y - sidebar_footer_height});
}

void sidebar_footer()
{
	::ScrollWhenDragging(); // drag to scroll the nav list, like the rest of the GUI
	ImGui::EndChild();      // TabsScroll
	// pin the footer to the bottom of the sidebar
	ImGui::SetCursorPosY(ImGui::GetContentRegionAvail().y + ImGui::GetCursorScreenPos().y - ImGui::GetWindowPos().y - sidebar_footer_height);
}

void end_sidebar()
{
	ImGui::PopStyleVar();   // ImGuiStyleVar_ItemSpacing
	ImGui::EndChild();      // Tabs
	ImGui::PopStyleVar();   // ImGuiStyleVar_WindowPadding
	ImGui::PopStyleColor(); // ImGuiCol_ChildBg
}

void shell_dividers(float top_bar_h, float tab_width)
{
	ImVec4 divider_col = current().border;
	divider_col.w *= background_alpha(); // follow the panel opacity
	const ImU32 divider = ImGui::GetColorU32(divider_col);
	const ImVec2 win_pos = ImGui::GetWindowPos();
	const ImVec2 win_size = ImGui::GetWindowSize();
	ImDrawList * dl = ImGui::GetWindowDrawList();
	dl->AddLine({win_pos.x, win_pos.y + top_bar_h}, {win_pos.x + win_size.x, win_pos.y + top_bar_h}, divider);
	dl->AddLine({win_pos.x + tab_width, win_pos.y + top_bar_h}, {win_pos.x + tab_width, win_pos.y + win_size.y}, divider);
}

// LVRR: the lobby's One window

namespace
{
ImVec2 sidebar_size;
float sidebar_footer_h = 0;
bool content_scrolled = false;
} // namespace

void shell_sidebar_begin(const ImVec2 & size, const std::string & title)
{
	const theme & t = current();
	const float pad = metrics::shell_sidebar_pad;
	sidebar_size = size;
	sidebar_footer_h = control_size() + 2 * pad;

	ImGui::PushStyleColor(ImGuiCol_ChildBg, ImVec4{0, 0, 0, 0});
	ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, {pad, 0});
	ImGui::BeginChild("Sidebar", size, ImGuiChildFlags_AlwaysUseWindowPadding, ImGuiWindowFlags_NoScrollbar | ImGuiWindowFlags_NoScrollWithMouse);
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	ImDrawList * draw = window->DrawList;
	const ImVec2 p = window->Pos;
	const float b = t.border_size;

	// flush with the window, inside its hairline, a hairline on the right
	draw->AddRectFilled(p + ImVec2{b, b}, p + size - ImVec2{0, b}, t.col(t.sidebar), t.window_rounding - b, ImDrawFlags_RoundCornersLeft);
	draw->AddRectFilled({p.x + size.x - b, p.y}, p + size, t.col(t.border));

	// app name where the headerbar is
	const float h = metrics::shell_header_height;
	ImGui::PushFont(bold_font(), ImGui::GetStyle().FontSizeBase * metrics::font_heading);
	const ImVec2 ts = ImGui::CalcTextSize(title.c_str());
	draw->AddText({p.x + pad + metrics::combo_padding.x, p.y + (h - ts.y) * 0.5f}, t.col(t.text), title.c_str());
	ImGui::PopFont();

	ImGui::SetCursorPosY(h);
	ImGui::PushStyleVar(ImGuiStyleVar_ItemSpacing, {metrics::card_item_spacing.x, 2 * metrics::one});
}

void shell_sidebar_footer()
{
	const float pad = metrics::shell_sidebar_pad;
	ImGui::SetCursorPos({pad + metrics::combo_padding.x * 0.5f, sidebar_size.y - sidebar_footer_h + pad});
}

void shell_sidebar_end()
{
	ImGui::PopStyleVar(); // ImGuiStyleVar_ItemSpacing
	ImGui::EndChild();
	ImGui::PopStyleVar();
	ImGui::PopStyleColor();
}

bool shell_header(float width, const std::string & title, const std::string & close_tooltip)
{
	const theme & t = current();
	const float h = metrics::shell_header_height;

	ImGui::PushStyleColor(ImGuiCol_ChildBg, ImVec4{0, 0, 0, 0});
	ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, {0, 0});
	ImGui::BeginChild("Header", {width, h}, 0, ImGuiWindowFlags_NoScrollbar | ImGuiWindowFlags_NoScrollWithMouse);
	ImGuiWindow * window = ImGui::GetCurrentWindow();
	ImDrawList * draw = window->DrawList;
	const ImVec2 p = window->Pos;

	// a hairline under the bar once the content scrolls under it
	if (content_scrolled)
		draw->AddRectFilled({p.x, p.y + h - t.border_size}, {p.x + width, p.y + h}, t.col(t.border));

	// centred title, clear of the close button; a longer one ends in an ellipsis
	ImGui::PushFont(bold_font(), ImGui::GetStyle().FontSizeBase * metrics::font_heading);
	const ImVec2 ts = ImGui::CalcTextSize(title.c_str());
	const float max_w = std::max(0.f, width - 2 * h);
	const float text_w = std::min(ts.x, max_w);
	const ImVec2 text_pos = {p.x + (width - text_w) * 0.5f, p.y + (h - ts.y) * 0.5f};
	if (text_w < ts.x)
	{
		// draws in the current font and ImGuiCol_Text; with no end it would stop at "##"
		ImGui::PushStyleColor(ImGuiCol_Text, t.text);
		ImGui::RenderTextEllipsis(draw, text_pos, {text_pos.x + text_w, p.y + h}, text_pos.x + text_w, title.c_str(), title.c_str() + title.size(), &ts);
		ImGui::PopStyleColor();
	}
	else
		draw->AddText(text_pos, t.col(t.text), title.c_str());
	ImGui::PopFont();

	// round close button
	const float d = metrics::shell_close_size;
	const ImVec2 center = {p.x + width - h * 0.5f, p.y + h * 0.5f};
	const ImRect bb(center - ImVec2{d, d} * 0.5f, center + ImVec2{d, d} * 0.5f);
	ImGui::SetCursorScreenPos(bb.Min);
	ImGui::ItemSize(bb);
	const ImGuiID id = window->GetID("close");
	bool pressed = false;
	if (ImGui::ItemAdd(bb, id))
	{
		bool hovered, held;
		pressed = ImGui::ButtonBehavior(bb, id, &hovered, &held);
		hover_haptic();
		if (hovered)
			show_tooltip(close_tooltip);
		draw->AddCircleFilled(center, d * 0.5f, t.col(held ? t.control_active : hovered ? t.control_hovered
		                                                                                : t.control));
		glyph_centered(draw, ICON_LVRR_CLOSE, center, metrics::shell_close_glyph, t.col(t.text));
	}

	ImGui::EndChild();
	ImGui::PopStyleVar();
	ImGui::PopStyleColor();
	return pressed;
}

void shell_content_begin(const ImVec2 & size)
{
	// a centred column on the canvas
	const ImVec2 pad = metrics::shell_content_padding;
	const float pad_x = std::max(pad.x, (size.x - metrics::shell_content_max_width) * 0.5f);

	ImGui::PushStyleColor(ImGuiCol_ChildBg, ImVec4{0, 0, 0, 0});
	ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, {pad_x, pad.y});
	ImGui::PushStyleVar(ImGuiStyleVar_ScrollbarSize, 6 * metrics::one);
	ImGui::BeginChild("Content", size, ImGuiChildFlags_AlwaysUseWindowPadding);
}

void shell_content_end()
{
	::ScrollWhenDragging();
	// room below the last box for its shadow and the corner
	ImGui::Dummy({0, metrics::shell_content_padding.y});
	content_scrolled = ImGui::GetScrollY() > 0;
	ImGui::EndChild();
	ImGui::PopStyleVar(2);
	ImGui::PopStyleColor();
}

} // namespace wivrn::ui
