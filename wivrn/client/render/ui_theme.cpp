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

#include "ui_theme.h"
#include "utils/i18n.h"

#include <cmath>
#include <cstdint>

namespace wivrn::ui
{

static ImVec4 rgb(int r, int g, int b, float a = 1)
{
	return {r / 255.f, g / 255.f, b / 255.f, a};
}

static theme dark_default()
{
	theme t;
	t.name = "Dark";
	t.localized_name = _C("theme name", "Dark");

	t.accent = rgb(30, 115, 255);
	t.accent_hovered = rgb(64, 140, 255);
	t.accent_active = rgb(18, 95, 228);
	t.on_accent = rgb(255, 255, 255);

	t.background = rgb(10, 11, 13);
	t.background_contrast = rgb(4, 5, 6);
	t.card = rgb(22, 24, 29);
	t.card_hovered = rgb(28, 30, 36);
	t.control = rgb(35, 38, 46);
	t.control_hovered = rgb(44, 48, 57);
	t.control_active = rgb(52, 57, 68);

	t.text = rgb(244, 245, 248);
	t.text_muted = rgb(150, 156, 167);
	t.heading = t.accent;
	t.border = rgb(40, 43, 51);

	t.danger = rgb(239, 68, 68);
	t.danger_hovered = rgb(248, 96, 96);
	t.success = rgb(52, 199, 123);
	t.warning = rgb(249, 115, 6);

	t.dimming = rgb(128, 128, 128, 0.5f);

	t.sidebar = t.background;
	t.surface_alt = t.card_hovered;
	t.text_faint = t.text_muted;
	t.hover = rgb(255, 255, 255, 0.06f);
	t.pressed = rgb(255, 255, 255, 0.1f);
	t.container = t.accent;
	t.on_container = t.on_accent;
	t.accent_soft = rgb(30, 115, 255, 0.16f);
	t.success_soft = rgb(52, 199, 123, 0.16f);
	t.warning_soft = rgb(249, 115, 6, 0.16f);
	t.danger_soft = rgb(239, 68, 68, 0.16f);

	t.rounding = 8;
	t.card_rounding = 14;
	t.window_rounding = 14;
	t.small_rounding = 6;
	t.border_size = 1;
	t.font_scale = 1.0; // user multiplier, 100% maps to metrics::font_base
	return t;
}

static theme midnight()
{
	theme t = dark_default();
	t.name = "Midnight";
	t.localized_name = _C("theme name", "Midnight");
	t.background = rgb(6, 8, 16);
	t.card = rgb(16, 20, 34);
	t.card_hovered = rgb(22, 27, 44);
	t.control = rgb(28, 34, 54);
	t.control_hovered = rgb(36, 43, 66);
	t.control_active = rgb(44, 52, 78);
	t.border = rgb(30, 37, 58);
	return t;
}

static theme slate_light()
{
	theme t = dark_default();
	t.name = "Slate";
	t.localized_name = _C("theme name", "Slate");
	t.background = rgb(28, 30, 36);
	t.card = rgb(38, 41, 49);
	t.card_hovered = rgb(46, 50, 60);
	t.control = rgb(54, 59, 70);
	t.control_hovered = rgb(64, 70, 83);
	t.control_active = rgb(74, 81, 96);
	t.text = rgb(236, 238, 242);
	t.text_muted = rgb(168, 174, 186);
	t.border = rgb(58, 63, 75);
	return t;
}

// True black, ideal for OLED panels
static theme oled()
{
	theme t = dark_default();
	t.name = "OLED";
	t.localized_name = _C("theme name", "OLED");
	t.background = rgb(0, 0, 0);
	t.card = rgb(6, 6, 8);
	t.card_hovered = rgb(11, 12, 14);
	t.control = rgb(22, 24, 28);
	t.control_hovered = rgb(30, 32, 38);
	t.control_active = rgb(40, 43, 50);
	t.border = rgb(26, 28, 33);
	return t;
}

// Neutral dark gray
static theme graphite()
{
	theme t = dark_default();
	t.name = "Graphite";
	t.localized_name = _C("theme name", "Graphite");
	t.background = rgb(20, 21, 23);
	t.card = rgb(31, 33, 36);
	t.card_hovered = rgb(39, 41, 45);
	t.control = rgb(48, 51, 56);
	t.control_hovered = rgb(58, 62, 68);
	t.control_active = rgb(68, 72, 80);
	t.text = rgb(238, 240, 243);
	t.text_muted = rgb(160, 164, 172);
	t.border = rgb(50, 53, 59);
	return t;
}

// Light surfaces, dark text
static theme light()
{
	theme t = dark_default();
	t.name = "Light";
	t.localized_name = _C("theme name", "Light");
	t.background = rgb(188, 193, 202);
	t.card = rgb(252, 252, 253);
	t.card_hovered = rgb(242, 243, 246);
	t.control = rgb(206, 210, 219);
	t.control_hovered = rgb(193, 198, 208);
	t.control_active = rgb(178, 184, 196);
	t.text = rgb(14, 16, 20);
	t.text_muted = rgb(66, 72, 84);
	t.border = rgb(176, 182, 194);
	t.success = rgb(36, 147, 90);
	return t;
}

// oneVR: alpha that darkens a light backdrop as much as black at alpha a does in sRGB, the GUI
// blends in linear light (see imgui_context::end_frame)
static float ink(float a)
{
	const float c = 1 - a;
	return 1 - (c <= 0.04045f ? c / 12.92f : std::pow((c + 0.055f) / 1.055f, 2.4f));
}

static ImVec4 black(float a)
{
	return {0, 0, 0, ink(a)};
}

static ImVec4 hex(uint32_t c, float a = 1)
{
	return rgb((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff, a);
}

// c at alpha a over an opaque backdrop, blended in sRGB like the One spec's CSS
static ImVec4 over(uint32_t c, float a, uint32_t backdrop)
{
	const ImVec4 f = hex(c), b = hex(backdrop);
	return {f.x * a + b.x * (1 - a), f.y * a + b.y * (1 - a), f.z * a + b.z * (1 - a), 1};
}

// oneVR: the oneDesign "One" language, light, blue accent, shared with the dashboard
static theme one()
{
	theme t = dark_default();
	t.name = "One";
	t.localized_name = "One";

	const uint32_t surface = 0xfcffff;

	t.accent = hex(0x2474cf);
	t.accent_hovered = hex(0x1467c2);
	t.accent_active = hex(0x005bb5);
	t.on_accent = hex(0xffffff);
	t.accent_soft = over(0x2474cf, 0x24 / 255.f, surface);
	t.container = hex(0xc5e8ff);
	t.on_container = hex(0x003b7e);

	t.background = hex(0xf4f7fb);          // canvas
	t.background_contrast = hex(0xf9fcff); // float: popups, menus, tooltips, opaque as nothing blurs
	t.sidebar = hex(0xeceff2);
	t.card = hex(surface);
	t.card_hovered = hex(0xf1f4f7);
	t.surface_alt = hex(0xf1f4f7);
	t.control = black(0x0f / 255.f);         // fill
	t.control_hovered = black(0x1f / 255.f); // fill_strong
	t.control_active = black(0x29 / 255.f);
	t.hover = black(0x0d / 255.f);
	t.pressed = black(0x17 / 255.f);

	t.text = hex(0x12161b);
	t.text_muted = hex(0x51565c);
	t.text_faint = hex(0x81878d);
	t.heading = t.text;
	t.border = black(0x1a / 255.f); // stroke

	t.danger = hex(0xd02b31);
	t.danger_hovered = hex(0xb8232a);
	t.success = hex(0x0e8c41);
	t.warning = hex(0xa96200);
	t.success_soft = over(0x0e8c41, 0x21 / 255.f, surface);
	t.warning_soft = over(0xa96200, 0x21 / 255.f, surface);
	t.danger_soft = over(0xd02b31, 0x21 / 255.f, surface);

	t.dimming = black(0.3f);

	t.rounding = 10 * metrics::one;
	t.card_rounding = 14 * metrics::one;
	t.window_rounding = 18 * metrics::one;
	t.small_rounding = 6 * metrics::one;
	t.border_size = metrics::one;
	return t;
}

std::vector<theme> presets()
{
	return {one(), oled(), dark_default(), midnight(), graphite(), slate_light(), light()};
}

ImFont *& bold_font()
{
	static ImFont * f = nullptr;
	return f;
}

ImFont *& semibold_font()
{
	static ImFont * f = nullptr;
	return f;
}

std::vector<accent_swatch> accent_swatches()
{
	return {
	        {"Blue", rgb(30, 115, 255), rgb(64, 140, 255), rgb(18, 95, 228)},
	        {"Violet", rgb(132, 72, 255), rgb(152, 98, 255), rgb(112, 52, 232)},
	        {"Emerald", rgb(0, 200, 132), rgb(28, 214, 150), rgb(0, 172, 112)},
	        {"Amber", rgb(255, 160, 0), rgb(255, 176, 36), rgb(228, 140, 0)},
	        {"Rose", rgb(255, 40, 82), rgb(255, 74, 108), rgb(230, 28, 68)},
	        {"Cyan", rgb(0, 190, 225), rgb(34, 205, 236), rgb(0, 168, 200)},
	};
}

void set_accent(const accent_swatch & swatch)
{
	theme & t = current();
	t.accent = swatch.base;
	t.accent_hovered = swatch.hovered;
	t.accent_active = swatch.active;
}

theme & current()
{
	static theme t = one();
	return t;
}

void set_theme(const theme & t)
{
	current() = t;
}

float & background_alpha()
{
	static float alpha = 0.75f; // translucent panels by default
	return alpha;
}

void theme::apply() const
{
	ImGuiStyle & style = ImGui::GetStyle();

	style.FontScaleMain = font_scale;
	style.WindowRounding = window_rounding;
	style.ChildRounding = card_rounding;
	style.PopupRounding = card_rounding;
	style.FrameRounding = rounding;
	style.GrabRounding = rounding;
	style.TabRounding = rounding;
	style.ScrollbarRounding = rounding;
	style.FrameBorderSize = 0; // filled controls, no outline
	style.WindowBorderSize = border_size;
	style.PopupBorderSize = border_size;
	style.DisabledAlpha = 0.45f;

	ImVec4 * c = style.Colors;
	c[ImGuiCol_Text] = text;
	c[ImGuiCol_TextDisabled] = text_muted;
	c[ImGuiCol_WindowBg] = background;
	c[ImGuiCol_ChildBg] = {0, 0, 0, 0};
	c[ImGuiCol_PopupBg] = background_contrast;
	c[ImGuiCol_Border] = border;
	c[ImGuiCol_BorderShadow] = {0, 0, 0, 0};
	c[ImGuiCol_FrameBg] = control;
	c[ImGuiCol_FrameBgHovered] = control_hovered;
	c[ImGuiCol_FrameBgActive] = control_active;
	c[ImGuiCol_Button] = control;
	c[ImGuiCol_ButtonHovered] = control_hovered;
	c[ImGuiCol_ButtonActive] = control_active;
	c[ImGuiCol_Header] = container;
	c[ImGuiCol_HeaderHovered] = hover;
	c[ImGuiCol_HeaderActive] = pressed;
	c[ImGuiCol_SliderGrab] = accent;
	c[ImGuiCol_SliderGrabActive] = accent_active;
	c[ImGuiCol_CheckMark] = accent;
	c[ImGuiCol_TextSelectedBg] = accent_soft;
	c[ImGuiCol_InputTextCursor] = accent;
	c[ImGuiCol_NavCursor] = accent;
	c[ImGuiCol_ScrollbarBg] = {0, 0, 0, 0};
	c[ImGuiCol_ScrollbarGrab] = control_hovered;
	c[ImGuiCol_ScrollbarGrabHovered] = control_active;
	c[ImGuiCol_ScrollbarGrabActive] = text_faint;
	c[ImGuiCol_Separator] = border;
}

} // namespace wivrn::ui
