/*
 * oneVR: motion vectors for the headset's frame synthesis, from optical flow between consecutive
 * streamed frames, so the runtime can show the stream at twice the rate it arrives
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

#include "stream_defoveator.h"
#include "vk/allocation.h"
#include "wivrn_packets.h"
#include "xr/swapchain.h"
#include <array>
#include <vector>
#include <vulkan/vulkan_raii.hpp>
#include <openxr/openxr.h>

namespace xr
{
class instance;
class session;
class system;
} // namespace xr

class stream_spacewarp
{
public:
	enum class api
	{
		none,
		frame_synthesis, // XR_EXT_frame_synthesis
		space_warp,      // XR_FB_space_warp
	};

	// The enabled extension, XR_EXT_frame_synthesis first
	static api supported(xr::instance &);

	// What a streamed frame shows for one eye
	struct frame
	{
		XrTime display_time;
		XrPosef pose;
		XrFovf fov;
	};

	stream_spacewarp(
	        vk::raii::Device & device,
	        xr::instance & instance,
	        xr::system & system,
	        xr::session & session,
	        XrViewConfigurationType viewconfig,
	        stream_defoveator & defoveator,
	        vk::Format color_format);

	stream_spacewarp(const stream_spacewarp &) = delete;
	stream_spacewarp & operator=(const stream_spacewarp &) = delete;

	// The next frame is not extrapolated from the ones before
	void reset();

	// After defoveator.defoveate(), in the same command buffer: acquires the motion vector and
	// depth images and fills them for the frames shown at display_time
	void record(
	        vk::raii::CommandBuffer & command_buffer,
	        const std::array<wivrn::to_headset::foveation_parameter, 2> & foveation,
	        const std::array<stream_defoveator::input, 2> & inputs,
	        const std::array<frame, 2> & frames,
	        XrTime display_time);

	// After the command buffer was submitted
	void release();

	api get_api() const
	{
		return api_;
	}

	// For the stream's projection layer, after record()
	std::array<XrCompositionLayerSpaceWarpInfoFB, 2> space_warp_views() const;
	std::array<XrFrameSynthesisInfoEXT, 2> frame_synthesis_views() const;

private:
	static constexpr uint32_t view_count = 2;
	// pyramid levels at most, the finest at the motion vector size
	static constexpr uint32_t max_levels = 5;

	vk::raii::Device & device;
	stream_defoveator & defoveator;
	api api_;

	// motion vector and depth size, and the frame drawn at twice that
	vk::Extent2D extent;
	vk::Extent2D color_extent;
	uint32_t levels;

	xr::swapchain motion_vectors;
	xr::swapchain depth;
	vk::Format depth_format;
	bool acquired = false;

	// each frame drawn small, one layer per view
	image_allocation color;
	vk::raii::ImageView color_view = nullptr;
	std::array<vk::raii::ImageView, view_count> color_layer_views{nullptr, nullptr};
	std::array<vk::raii::Framebuffer, view_count> color_framebuffers{nullptr, nullptr};

	struct pyramid
	{
		image_allocation image;
		vk::raii::ImageView sampled = nullptr;
		std::vector<vk::raii::ImageView> storage; // one per level
	};
	// the luma of the last frame and of the one before, which swap
	std::array<pyramid, 2> luma;
	// flow at each level, finest last written
	pyramid flow;

	vk::raii::Sampler sampler = nullptr;
	buffer_allocation params;

	vk::raii::DescriptorPool ds_pool = nullptr;

	struct pass
	{
		vk::raii::DescriptorSetLayout ds_layout = nullptr;
		vk::raii::PipelineLayout layout = nullptr;
		vk::raii::Pipeline pipeline = nullptr;
	};
	pass luma_pass;
	pass downsample_pass;
	pass flow_pass;
	pass output_pass;

	// [pyramid]
	std::array<vk::DescriptorSet, 2> luma_ds;
	// [pyramid][level - 1]
	std::array<std::vector<vk::DescriptorSet>, 2> downsample_ds;
	// [pyramid holding the current frame][level]
	std::array<std::vector<vk::DescriptorSet>, 2> flow_ds;
	vk::DescriptorSet output_ds;

	vk::raii::RenderPass motion_vector_renderpass = nullptr;
	vk::raii::RenderPass depth_renderpass = nullptr;
	std::vector<vk::raii::ImageView> swapchain_views;
	// [image * view_count + view]
	std::vector<vk::raii::Framebuffer> motion_vector_framebuffers;
	std::vector<vk::raii::Framebuffer> depth_framebuffers;
	uint32_t motion_vector_index = 0;
	uint32_t depth_index = 0;

	// the pyramid holding the last frame
	int current = 0;
	bool initialized = false;
	bool have_previous = false;
	std::array<frame, 2> previous{};
	XrTime last_submitted = 0;
	// the last record() wrote no motion
	bool skip = true;

	pyramid make_pyramid(const char * name);
	vk::raii::Pipeline make_compute_pipeline(const pass &, const char * shader);
	pass make_pass(std::span<const vk::DescriptorSetLayoutBinding> bindings, vk::ShaderStageFlags push_constant_stages, uint32_t push_constant_size);
	void make_output_pipeline();
	void make_framebuffers();
	void write_descriptors();
	void write_params(const std::array<frame, 2> & frames, float time_scale);
};
