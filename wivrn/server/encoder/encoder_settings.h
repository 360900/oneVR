/*
 * WiVRn VR streaming
 * Copyright (C) 2022  Guillaume Meunier <guillaume.meunier@centraliens.net>
 * Copyright (C) 2022  Patrick Nicolas <patricknicolas@laposte.net>
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

#include "wivrn_packets.h"

#include <map>
#include <optional>
#include <string>
#include <string_view>

namespace wivrn
{
struct vk_bundle;
class wivrn_session;

/// Whether the configuration asks for AV1 with the Vulkan encoder. The headset's codec list
/// starts with AV1, and Vulkan AV1 gives no image on NVIDIA and loses the device on RDNA3.5, so
/// this is the only case the Vulkan encoder uses AV1.
inline bool vulkan_av1_requested(std::string_view encoder, std::optional<video_codec> codec)
{
	return encoder == "vulkan" and codec == video_codec::av1;
}

struct encoder_settings
{
	uint16_t width;
	uint16_t height;
	video_codec codec; // left, right, alpha
	float fps;
	// encoder identifier, such as nvenc, vaapi or x264
	std::string encoder_name;
	uint64_t bitrate;                           // bit/s
	double bitrate_multiplier;                  // encoder bitrate / global bitrate
	std::map<std::string, std::string> options; // additional encoder-specific configuration
	int bit_depth;
	std::optional<std::string> device;
};

std::array<encoder_settings, 3> get_encoder_settings(wivrn::vk_bundle &, wivrn_session &);

void print_encoders(const std::array<wivrn::encoder_settings, 3> & encoders);

} // namespace wivrn
