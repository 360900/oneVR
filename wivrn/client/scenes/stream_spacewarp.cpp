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

#include "stream_spacewarp.h"

#include "application.h"
#include "utils/contains.h"
#include "vk/pipeline.h"
#include "vk/shader.h"
#include "xr/instance.h"
#include "xr/session.h"
#include "xr/system.h"
#include <algorithm>
#include <cmath>
#include <glm/glm.hpp>
#include <glm/gtc/quaternion.hpp>
#include <ranges>
#include <spdlog/spdlog.h>
#include <stdexcept>

namespace
{
// a frame older than this is too far back to tell how things move
constexpr XrDuration max_frame_gap = 100'000'000;

// the head's motion takes over where the flow strays this far, per image width, for one
// pixel's worth of luma difference
constexpr float smoothness = 0.3;
// mean luma difference over a patch beyond which the flow is not trusted
constexpr float match_threshold = 0.08;

// depth the runtime reprojects with: the far plane everywhere, so it only corrects head
// rotation, as it does without depth
constexpr float near_z = 0.1;
constexpr float far_z = 1000;

struct view_params
{
	glm::mat4 rotation;
	glm::vec4 tan_current;
	glm::vec4 tan_previous;
	glm::vec4 misc;
};

struct params_t
{
	view_params views[2];
};

struct flow_pc
{
	int32_t level;
	int32_t coarsest;
	float smoothness;
};

struct output_pc
{
	int32_t layer;
	int32_t mode;
	float threshold;
};

glm::quat to_glm(const XrQuaternionf & q)
{
	return glm::quat(q.w, q.x, q.y, q.z);
}

glm::vec4 tangents(const XrFovf & fov)
{
	return {std::tan(fov.angleLeft), std::tan(fov.angleRight), std::tan(fov.angleUp), std::tan(fov.angleDown)};
}

vk::Extent2D level_extent(vk::Extent2D base, uint32_t level)
{
	return {std::max(1u, base.width >> level), std::max(1u, base.height >> level)};
}

uint32_t groups(uint32_t size)
{
	return (size + 7) / 8;
}

bool has_stencil(vk::Format format)
{
	switch (format)
	{
		case vk::Format::eD16UnormS8Uint:
		case vk::Format::eD24UnormS8Uint:
		case vk::Format::eD32SfloatS8Uint:
			return true;
		default:
			return false;
	}
}

void compute_barrier(vk::raii::CommandBuffer & command_buffer, vk::PipelineStageFlags dst_stage = vk::PipelineStageFlagBits::eComputeShader)
{
	command_buffer.pipelineBarrier(
	        vk::PipelineStageFlagBits::eComputeShader,
	        dst_stage,
	        {},
	        vk::MemoryBarrier{
	                .srcAccessMask = vk::AccessFlagBits::eShaderWrite,
	                .dstAccessMask = vk::AccessFlagBits::eShaderRead,
	        },
	        {},
	        {});
}

const XrPosef identity_pose{
        .orientation = {0, 0, 0, 1},
        .position = {0, 0, 0},
};
} // namespace

stream_spacewarp::api stream_spacewarp::supported(xr::instance & instance)
{
	if (instance.has_extension(XR_EXT_FRAME_SYNTHESIS_EXTENSION_NAME))
		return api::frame_synthesis;
	if (instance.has_extension(XR_FB_SPACE_WARP_EXTENSION_NAME))
		return api::space_warp;
	return api::none;
}

stream_spacewarp::stream_spacewarp(
        vk::raii::Device & device,
        xr::instance & instance,
        xr::system & system,
        xr::session & session,
        XrViewConfigurationType viewconfig,
        stream_defoveator & defoveator,
        vk::Format color_format) :
        device(device),
        defoveator(defoveator),
        api_(supported(instance))
{
	if (api_ == api::none)
		throw std::runtime_error("no frame synthesis extension");

	XrExtent2Di recommended{};
	if (api_ == api::frame_synthesis)
	{
		auto views = system.frame_synthesis_config_views(viewconfig);
		if (not views.empty())
			recommended = {int32_t(views[0].recommendedMotionVectorImageRectWidth), int32_t(views[0].recommendedMotionVectorImageRectHeight)};
	}
	else
	{
		auto properties = system.space_warp_properties();
		recommended = {int32_t(properties.recommendedMotionVectorImageRectWidth), int32_t(properties.recommendedMotionVectorImageRectHeight)};
	}
	if (recommended.width <= 0 or recommended.height <= 0)
	{
		// about what Meta recommends, a quarter of the eye in each direction
		auto view = system.view_configuration_views(viewconfig)[0];
		recommended = {int32_t(view.recommendedImageRectWidth / 4), int32_t(view.recommendedImageRectHeight / 4)};
	}
	extent = vk::Extent2D{uint32_t(std::max(recommended.width, 16)), uint32_t(std::max(recommended.height, 16))};
	color_extent = vk::Extent2D{extent.width * 2, extent.height * 2};

	// down to about 16 pixels, enough to find large motion
	levels = 1;
	while (levels < max_levels and std::min(extent.width, extent.height) >> levels >= 12)
		++levels;

	const auto formats = session.get_swapchain_formats();
	if (not utils::contains(formats, vk::Format::eR16G16B16A16Sfloat))
		throw std::runtime_error("no 16-bit float swapchain for motion vectors");

	depth_format = vk::Format::eUndefined;
	for (auto format: {vk::Format::eD16Unorm, vk::Format::eD24UnormS8Uint, vk::Format::eX8D24UnormPack32, vk::Format::eD32Sfloat, vk::Format::eD32SfloatS8Uint})
	{
		if (utils::contains(formats, format))
		{
			depth_format = format;
			break;
		}
	}
	if (depth_format == vk::Format::eUndefined)
		throw std::runtime_error("no depth swapchain format");

	motion_vectors = xr::swapchain(instance, session, device, vk::Format::eR16G16B16A16Sfloat, extent.width, extent.height, 1, view_count);
	depth = xr::swapchain(instance, session, device, depth_format, extent.width, extent.height, 1, view_count);

	color = image_allocation{
	        device,
	        vk::ImageCreateInfo{
	                .imageType = vk::ImageType::e2D,
	                .format = color_format,
	                .extent = {color_extent.width, color_extent.height, 1},
	                .mipLevels = 1,
	                .arrayLayers = view_count,
	                .samples = vk::SampleCountFlagBits::e1,
	                .tiling = vk::ImageTiling::eOptimal,
	                .usage = vk::ImageUsageFlagBits::eColorAttachment | vk::ImageUsageFlagBits::eSampled,
	        },
	        VmaAllocationCreateInfo{
	                .usage = VMA_MEMORY_USAGE_AUTO,
	        },
	        "space warp frame"};

	color_view = vk::raii::ImageView(
	        device,
	        vk::ImageViewCreateInfo{
	                .image = vk::Image(color),
	                .viewType = vk::ImageViewType::e2DArray,
	                .format = color_format,
	                .subresourceRange = {
	                        .aspectMask = vk::ImageAspectFlagBits::eColor,
	                        .levelCount = 1,
	                        .layerCount = view_count,
	                },
	        });

	for (uint32_t view = 0; view < view_count; ++view)
	{
		// like the defoveator's own targets, so its render pass draws into them
		color_layer_views[view] = vk::raii::ImageView(
		        device,
		        vk::ImageViewCreateInfo{
		                .image = vk::Image(color),
		                .viewType = vk::ImageViewType::e2DArray,
		                .format = color_format,
		                .subresourceRange = {
		                        .aspectMask = vk::ImageAspectFlagBits::eColor,
		                        .levelCount = 1,
		                        .baseArrayLayer = view,
		                        .layerCount = 1,
		                },
		        });

		vk::FramebufferCreateInfo fb_info{
		        .renderPass = defoveator.render_pass(),
		        .width = color_extent.width,
		        .height = color_extent.height,
		        .layers = 1,
		};
		fb_info.setAttachments(*color_layer_views[view]);
		color_framebuffers[view] = vk::raii::Framebuffer(device, fb_info);
	}

	luma[0] = make_pyramid("space warp luma 0");
	luma[1] = make_pyramid("space warp luma 1");
	flow = make_pyramid("space warp flow");

	sampler = vk::raii::Sampler(
	        device,
	        vk::SamplerCreateInfo{
	                .magFilter = vk::Filter::eLinear,
	                .minFilter = vk::Filter::eLinear,
	                .mipmapMode = vk::SamplerMipmapMode::eNearest,
	                .addressModeU = vk::SamplerAddressMode::eClampToEdge,
	                .addressModeV = vk::SamplerAddressMode::eClampToEdge,
	                .addressModeW = vk::SamplerAddressMode::eClampToEdge,
	                .minLod = 0,
	                .maxLod = float(levels),
	        });

	params = buffer_allocation(
	        device,
	        vk::BufferCreateInfo{
	                .size = sizeof(params_t),
	                .usage = vk::BufferUsageFlagBits::eUniformBuffer,
	        },
	        VmaAllocationCreateInfo{
	                .flags = VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT,
	                .usage = VMA_MEMORY_USAGE_AUTO,
	                .requiredFlags = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
	        },
	        "space warp parameters");
	*reinterpret_cast<params_t *>(params.map()) = {};

	const uint32_t sets = 2 + 2 * (levels - 1) + 2 * levels + 1;
	std::array pool_sizes{
	        vk::DescriptorPoolSize{
	                .type = vk::DescriptorType::eCombinedImageSampler,
	                .descriptorCount = 2 + 2 * levels * 3 + 1,
	        },
	        vk::DescriptorPoolSize{
	                .type = vk::DescriptorType::eStorageImage,
	                .descriptorCount = 2 + 2 * (levels - 1) * 2 + 2 * levels,
	        },
	        vk::DescriptorPoolSize{
	                .type = vk::DescriptorType::eUniformBuffer,
	                .descriptorCount = 2 * levels + 1,
	        },
	};
	ds_pool = vk::raii::DescriptorPool(
	        device,
	        vk::DescriptorPoolCreateInfo{
	                .maxSets = sets,
	                .poolSizeCount = pool_sizes.size(),
	                .pPoolSizes = pool_sizes.data(),
	        });

	const auto sampled = [](uint32_t binding, vk::ShaderStageFlags stages) {
		return vk::DescriptorSetLayoutBinding{
		        .binding = binding,
		        .descriptorType = vk::DescriptorType::eCombinedImageSampler,
		        .descriptorCount = 1,
		        .stageFlags = stages,
		};
	};
	const auto storage = [](uint32_t binding) {
		return vk::DescriptorSetLayoutBinding{
		        .binding = binding,
		        .descriptorType = vk::DescriptorType::eStorageImage,
		        .descriptorCount = 1,
		        .stageFlags = vk::ShaderStageFlagBits::eCompute,
		};
	};
	const auto uniform = [](uint32_t binding, vk::ShaderStageFlags stages) {
		return vk::DescriptorSetLayoutBinding{
		        .binding = binding,
		        .descriptorType = vk::DescriptorType::eUniformBuffer,
		        .descriptorCount = 1,
		        .stageFlags = stages,
		};
	};
	const auto compute = vk::ShaderStageFlagBits::eCompute;
	const auto fragment = vk::ShaderStageFlagBits::eFragment;

	luma_pass = make_pass(std::array{sampled(0, compute), storage(1)}, {}, 0);
	luma_pass.pipeline = make_compute_pipeline(luma_pass, "spacewarp_luma.comp");

	downsample_pass = make_pass(std::array{storage(0), storage(1)}, {}, 0);
	downsample_pass.pipeline = make_compute_pipeline(downsample_pass, "spacewarp_downsample.comp");

	flow_pass = make_pass(std::array{sampled(0, compute), sampled(1, compute), sampled(2, compute), storage(3), uniform(4, compute)}, compute, sizeof(flow_pc));
	flow_pass.pipeline = make_compute_pipeline(flow_pass, "spacewarp_flow.comp");

	output_pass = make_pass(std::array{sampled(0, fragment), uniform(4, fragment)}, vk::ShaderStageFlagBits::eVertex | fragment, sizeof(output_pc));

	// motion vectors, written by a fragment shader so the swapchain needs no extra usage
	{
		vk::AttachmentDescription attachment{
		        .format = vk::Format::eR16G16B16A16Sfloat,
		        .samples = vk::SampleCountFlagBits::e1,
		        .loadOp = vk::AttachmentLoadOp::eDontCare,
		        .storeOp = vk::AttachmentStoreOp::eStore,
		        .stencilLoadOp = vk::AttachmentLoadOp::eDontCare,
		        .stencilStoreOp = vk::AttachmentStoreOp::eDontCare,
		        .initialLayout = vk::ImageLayout::eUndefined,
		        .finalLayout = vk::ImageLayout::eColorAttachmentOptimal,
		};
		vk::AttachmentReference ref{
		        .attachment = 0,
		        .layout = vk::ImageLayout::eColorAttachmentOptimal,
		};
		vk::SubpassDescription subpass{
		        .pipelineBindPoint = vk::PipelineBindPoint::eGraphics,
		        .colorAttachmentCount = 1,
		        .pColorAttachments = &ref,
		};
		motion_vector_renderpass = vk::raii::RenderPass(
		        device,
		        vk::RenderPassCreateInfo{
		                .attachmentCount = 1,
		                .pAttachments = &attachment,
		                .subpassCount = 1,
		                .pSubpasses = &subpass,
		        });
	}

	// depth, only cleared
	{
		vk::AttachmentDescription attachment{
		        .format = depth_format,
		        .samples = vk::SampleCountFlagBits::e1,
		        .loadOp = vk::AttachmentLoadOp::eClear,
		        .storeOp = vk::AttachmentStoreOp::eStore,
		        .stencilLoadOp = has_stencil(depth_format) ? vk::AttachmentLoadOp::eClear : vk::AttachmentLoadOp::eDontCare,
		        .stencilStoreOp = has_stencil(depth_format) ? vk::AttachmentStoreOp::eStore : vk::AttachmentStoreOp::eDontCare,
		        .initialLayout = vk::ImageLayout::eUndefined,
		        .finalLayout = vk::ImageLayout::eDepthStencilAttachmentOptimal,
		};
		vk::AttachmentReference ref{
		        .attachment = 0,
		        .layout = vk::ImageLayout::eDepthStencilAttachmentOptimal,
		};
		vk::SubpassDescription subpass{
		        .pipelineBindPoint = vk::PipelineBindPoint::eGraphics,
		        .pDepthStencilAttachment = &ref,
		};
		depth_renderpass = vk::raii::RenderPass(
		        device,
		        vk::RenderPassCreateInfo{
		                .attachmentCount = 1,
		                .pAttachments = &attachment,
		                .subpassCount = 1,
		                .pSubpasses = &subpass,
		        });
	}

	make_output_pipeline();
	make_framebuffers();
	write_descriptors();

	spdlog::info("Space warp with {}: {}x{} motion vectors, {} levels, depth {}",
	             api_ == api::frame_synthesis ? XR_EXT_FRAME_SYNTHESIS_EXTENSION_NAME : XR_FB_SPACE_WARP_EXTENSION_NAME,
	             extent.width,
	             extent.height,
	             levels,
	             vk::to_string(depth_format));
}

stream_spacewarp::pyramid stream_spacewarp::make_pyramid(const char * name)
{
	pyramid p{
	        .image = image_allocation{
	                device,
	                vk::ImageCreateInfo{
	                        .imageType = vk::ImageType::e2D,
	                        .format = vk::Format::eR16G16B16A16Sfloat,
	                        .extent = {extent.width, extent.height, 1},
	                        .mipLevels = levels,
	                        .arrayLayers = view_count,
	                        .samples = vk::SampleCountFlagBits::e1,
	                        .tiling = vk::ImageTiling::eOptimal,
	                        .usage = vk::ImageUsageFlagBits::eStorage | vk::ImageUsageFlagBits::eSampled,
	                },
	                VmaAllocationCreateInfo{
	                        .usage = VMA_MEMORY_USAGE_AUTO,
	                },
	                name},
	};

	p.sampled = vk::raii::ImageView(
	        device,
	        vk::ImageViewCreateInfo{
	                .image = vk::Image(p.image),
	                .viewType = vk::ImageViewType::e2DArray,
	                .format = vk::Format::eR16G16B16A16Sfloat,
	                .subresourceRange = {
	                        .aspectMask = vk::ImageAspectFlagBits::eColor,
	                        .levelCount = levels,
	                        .layerCount = view_count,
	                },
	        });

	for (uint32_t level = 0; level < levels; ++level)
	{
		p.storage.emplace_back(
		        device,
		        vk::ImageViewCreateInfo{
		                .image = vk::Image(p.image),
		                .viewType = vk::ImageViewType::e2DArray,
		                .format = vk::Format::eR16G16B16A16Sfloat,
		                .subresourceRange = {
		                        .aspectMask = vk::ImageAspectFlagBits::eColor,
		                        .baseMipLevel = level,
		                        .levelCount = 1,
		                        .layerCount = view_count,
		                },
		        });
	}
	return p;
}

stream_spacewarp::pass stream_spacewarp::make_pass(std::span<const vk::DescriptorSetLayoutBinding> bindings, vk::ShaderStageFlags push_constant_stages, uint32_t push_constant_size)
{
	pass p;
	p.ds_layout = vk::raii::DescriptorSetLayout(
	        device,
	        vk::DescriptorSetLayoutCreateInfo{
	                .bindingCount = uint32_t(bindings.size()),
	                .pBindings = bindings.data(),
	        });

	vk::PushConstantRange range{
	        .stageFlags = push_constant_stages,
	        .size = push_constant_size,
	};
	p.layout = vk::raii::PipelineLayout(
	        device,
	        vk::PipelineLayoutCreateInfo{
	                .setLayoutCount = 1,
	                .pSetLayouts = &*p.ds_layout,
	                .pushConstantRangeCount = push_constant_size ? 1u : 0u,
	                .pPushConstantRanges = push_constant_size ? &range : nullptr,
	        });
	return p;
}

vk::raii::Pipeline stream_spacewarp::make_compute_pipeline(const pass & p, const char * name)
{
	auto shader = load_shader(device, name);
	return vk::raii::Pipeline(
	        device,
	        application::get_pipeline_cache(),
	        vk::ComputePipelineCreateInfo{
	                .stage = {
	                        .stage = vk::ShaderStageFlagBits::eCompute,
	                        .module = *shader,
	                        .pName = "main",
	                },
	                .layout = *p.layout,
	        });
}

void stream_spacewarp::make_output_pipeline()
{
	auto vertex_shader = load_shader(device, "spacewarp_output.vert");
	auto fragment_shader = load_shader(device, "spacewarp_output.frag");

	vk::pipeline_builder pipeline_info{
	        .flags = {},
	        .Stages = {
	                {
	                        .stage = vk::ShaderStageFlagBits::eVertex,
	                        .module = *vertex_shader,
	                        .pName = "main",
	                },
	                {
	                        .stage = vk::ShaderStageFlagBits::eFragment,
	                        .module = *fragment_shader,
	                        .pName = "main",
	                },
	        },
	        .InputAssemblyState = {{
	                .topology = vk::PrimitiveTopology::eTriangleList,
	        }},
	        .Viewports = {{}},
	        .Scissors = {{}},
	        .RasterizationState = {{
	                .polygonMode = vk::PolygonMode::eFill,
	                .cullMode = vk::CullModeFlagBits::eNone,
	                .lineWidth = 1,
	        }},
	        .MultisampleState = {{
	                .rasterizationSamples = vk::SampleCountFlagBits::e1,
	        }},
	        .ColorBlendState = {.flags = {}},
	        .ColorBlendAttachments = {{
	                .colorWriteMask = vk::ColorComponentFlagBits::eR | vk::ColorComponentFlagBits::eG | vk::ColorComponentFlagBits::eB | vk::ColorComponentFlagBits::eA,
	        }},
	        .DynamicStates = {vk::DynamicState::eViewport, vk::DynamicState::eScissor},
	        .layout = *output_pass.layout,
	        .renderPass = *motion_vector_renderpass,
	        .subpass = 0,
	};

	output_pass.pipeline = device.createGraphicsPipeline(application::get_pipeline_cache(), pipeline_info);
}

void stream_spacewarp::make_framebuffers()
{
	const auto add = [&](const xr::swapchain & swapchain, vk::Format format, vk::ImageAspectFlags aspect, vk::RenderPass renderpass, std::vector<vk::raii::Framebuffer> & out) {
		for (vk::Image image: swapchain.images())
		{
			for (uint32_t view = 0; view < view_count; ++view)
			{
				auto & iv = swapchain_views.emplace_back(
				        device,
				        vk::ImageViewCreateInfo{
				                .image = image,
				                .viewType = vk::ImageViewType::e2D,
				                .format = format,
				                .subresourceRange = {
				                        .aspectMask = aspect,
				                        .levelCount = 1,
				                        .baseArrayLayer = view,
				                        .layerCount = 1,
				                },
				        });

				vk::FramebufferCreateInfo fb_info{
				        .renderPass = renderpass,
				        .width = extent.width,
				        .height = extent.height,
				        .layers = 1,
				};
				fb_info.setAttachments(*iv);
				out.emplace_back(device, fb_info);
			}
		}
	};

	swapchain_views.reserve((motion_vectors.images().size() + depth.images().size()) * view_count);
	add(motion_vectors, vk::Format::eR16G16B16A16Sfloat, vk::ImageAspectFlagBits::eColor, *motion_vector_renderpass, motion_vector_framebuffers);
	add(depth,
	    depth_format,
	    has_stencil(depth_format) ? vk::ImageAspectFlagBits::eDepth | vk::ImageAspectFlagBits::eStencil : vk::ImageAspectFlagBits::eDepth,
	    *depth_renderpass,
	    depth_framebuffers);
}

void stream_spacewarp::write_descriptors()
{
	const auto allocate = [&](const pass & p) {
		return device.allocateDescriptorSets(vk::DescriptorSetAllocateInfo{
		        .descriptorPool = *ds_pool,
		        .descriptorSetCount = 1,
		        .pSetLayouts = &*p.ds_layout,
		})[0]
		        .release();
	};

	// every image stays in the general layout, written and read in turn
	const auto sampled = [&](vk::ImageView view, vk::ImageLayout layout = vk::ImageLayout::eGeneral) {
		return vk::DescriptorImageInfo{
		        .sampler = *sampler,
		        .imageView = view,
		        .imageLayout = layout,
		};
	};
	const auto storage = [](vk::ImageView view) {
		return vk::DescriptorImageInfo{
		        .imageView = view,
		        .imageLayout = vk::ImageLayout::eGeneral,
		};
	};
	const vk::DescriptorBufferInfo params_info{
	        .buffer = vk::Buffer(params),
	        .range = sizeof(params_t),
	};

	const auto image_write = [](vk::DescriptorSet ds, uint32_t binding, vk::DescriptorType type, const vk::DescriptorImageInfo & info) {
		return vk::WriteDescriptorSet{
		        .dstSet = ds,
		        .dstBinding = binding,
		        .descriptorCount = 1,
		        .descriptorType = type,
		        .pImageInfo = &info,
		};
	};
	const auto params_write = [&](vk::DescriptorSet ds) {
		return vk::WriteDescriptorSet{
		        .dstSet = ds,
		        .dstBinding = 4,
		        .descriptorCount = 1,
		        .descriptorType = vk::DescriptorType::eUniformBuffer,
		        .pBufferInfo = &params_info,
		};
	};

	for (int p = 0; p < 2; ++p)
	{
		luma_ds[p] = allocate(luma_pass);
		auto color_info = sampled(*color_view, vk::ImageLayout::eShaderReadOnlyOptimal);
		auto luma_info = storage(*luma[p].storage[0]);
		device.updateDescriptorSets(
		        {
		                image_write(luma_ds[p], 0, vk::DescriptorType::eCombinedImageSampler, color_info),
		                image_write(luma_ds[p], 1, vk::DescriptorType::eStorageImage, luma_info),
		        },
		        {});

		for (uint32_t level = 1; level < levels; ++level)
		{
			auto ds = downsample_ds[p].emplace_back(allocate(downsample_pass));
			auto src = storage(*luma[p].storage[level - 1]);
			auto dst = storage(*luma[p].storage[level]);
			device.updateDescriptorSets(
			        {
			                image_write(ds, 0, vk::DescriptorType::eStorageImage, src),
			                image_write(ds, 1, vk::DescriptorType::eStorageImage, dst),
			        },
			        {});
		}

		for (uint32_t level = 0; level < levels; ++level)
		{
			auto ds = flow_ds[p].emplace_back(allocate(flow_pass));
			auto current_info = sampled(*luma[p].sampled);
			auto previous_info = sampled(*luma[1 - p].sampled);
			auto coarser_info = sampled(*flow.sampled);
			auto out_info = storage(*flow.storage[level]);
			device.updateDescriptorSets(
			        {
			                image_write(ds, 0, vk::DescriptorType::eCombinedImageSampler, current_info),
			                image_write(ds, 1, vk::DescriptorType::eCombinedImageSampler, previous_info),
			                image_write(ds, 2, vk::DescriptorType::eCombinedImageSampler, coarser_info),
			                image_write(ds, 3, vk::DescriptorType::eStorageImage, out_info),
			                params_write(ds),
			        },
			        {});
		}
	}

	output_ds = allocate(output_pass);
	auto flow_info = sampled(*flow.sampled);
	device.updateDescriptorSets(
	        {
	                image_write(output_ds, 0, vk::DescriptorType::eCombinedImageSampler, flow_info),
	                params_write(output_ds),
	        },
	        {});
}

void stream_spacewarp::reset()
{
	have_previous = false;
	last_submitted = 0;
}

void stream_spacewarp::write_params(const std::array<frame, 2> & frames, float time_scale)
{
	auto & p = *reinterpret_cast<params_t *>(params.map());
	for (uint32_t view = 0; view < view_count; ++view)
	{
		// the current view's directions in the previous view
		glm::quat rotation = glm::inverse(to_glm(previous[view].pose.orientation)) * to_glm(frames[view].pose.orientation);
		p.views[view] = {
		        .rotation = glm::mat4_cast(rotation),
		        .tan_current = tangents(frames[view].fov),
		        .tan_previous = tangents(previous[view].fov),
		        .misc = {time_scale, 0, 0, 0},
		};
	}
}

void stream_spacewarp::record(
        vk::raii::CommandBuffer & command_buffer,
        const std::array<wivrn::to_headset::foveation_parameter, 2> & foveation,
        const std::array<stream_defoveator::input, 2> & inputs,
        const std::array<frame, 2> & frames,
        XrTime display_time)
{
	motion_vector_index = motion_vectors.acquire();
	depth_index = depth.acquire();
	acquired = true;
	motion_vectors.wait();
	depth.wait();

	if (not initialized)
	{
		std::array<vk::ImageMemoryBarrier, 3> barriers;
		for (auto [barrier, image]: std::views::zip(barriers, std::array{vk::Image(luma[0].image), vk::Image(luma[1].image), vk::Image(flow.image)}))
		{
			barrier = vk::ImageMemoryBarrier{
			        .srcAccessMask = vk::AccessFlagBits::eNone,
			        .dstAccessMask = vk::AccessFlagBits::eShaderRead | vk::AccessFlagBits::eShaderWrite,
			        .oldLayout = vk::ImageLayout::eUndefined,
			        .newLayout = vk::ImageLayout::eGeneral,
			        .image = image,
			        .subresourceRange = {
			                .aspectMask = vk::ImageAspectFlagBits::eColor,
			                .levelCount = levels,
			                .layerCount = view_count,
			        },
			};
		}
		command_buffer.pipelineBarrier(vk::PipelineStageFlagBits::eTopOfPipe, vk::PipelineStageFlagBits::eComputeShader | vk::PipelineStageFlagBits::eFragmentShader, {}, {}, {}, barriers);
		initialized = true;
	}

	bool is_new = not have_previous;
	for (uint32_t view = 0; view < view_count; ++view)
		is_new = is_new or frames[view].display_time != previous[view].display_time;

	bool extrapolate = false;
	if (is_new)
	{
		// how far things moved over the streamed interval, scaled to the submitted one
		float time_scale = 0;
		if (have_previous and last_submitted)
		{
			bool usable = true;
			for (uint32_t view = 0; view < view_count; ++view)
			{
				const XrDuration gap = frames[view].display_time - previous[view].display_time;
				usable = usable and gap > 0 and gap < max_frame_gap;
			}
			if (usable)
			{
				const XrDuration streamed = frames[0].display_time - previous[0].display_time;
				const XrDuration submitted = display_time - last_submitted;
				time_scale = std::clamp(float(submitted) / float(streamed), 0.f, 2.f);
			}
		}
		extrapolate = time_scale > 0;

		if (extrapolate)
			write_params(frames, time_scale);

		const int next = 1 - current;

		defoveator.redraw(command_buffer, foveation, inputs, {*color_framebuffers[0], *color_framebuffers[1]}, color_extent);

		command_buffer.pipelineBarrier(
		        vk::PipelineStageFlagBits::eColorAttachmentOutput,
		        vk::PipelineStageFlagBits::eComputeShader,
		        {},
		        {},
		        {},
		        vk::ImageMemoryBarrier{
		                .srcAccessMask = vk::AccessFlagBits::eColorAttachmentWrite,
		                .dstAccessMask = vk::AccessFlagBits::eShaderRead,
		                .oldLayout = vk::ImageLayout::eColorAttachmentOptimal,
		                .newLayout = vk::ImageLayout::eShaderReadOnlyOptimal,
		                .image = vk::Image(color),
		                .subresourceRange = {
		                        .aspectMask = vk::ImageAspectFlagBits::eColor,
		                        .levelCount = 1,
		                        .layerCount = view_count,
		                },
		        });

		command_buffer.bindPipeline(vk::PipelineBindPoint::eCompute, *luma_pass.pipeline);
		command_buffer.bindDescriptorSets(vk::PipelineBindPoint::eCompute, *luma_pass.layout, 0, luma_ds[next], {});
		command_buffer.dispatch(groups(extent.width), groups(extent.height), view_count);

		command_buffer.bindPipeline(vk::PipelineBindPoint::eCompute, *downsample_pass.pipeline);
		for (uint32_t level = 1; level < levels; ++level)
		{
			compute_barrier(command_buffer);
			auto size = level_extent(extent, level);
			command_buffer.bindDescriptorSets(vk::PipelineBindPoint::eCompute, *downsample_pass.layout, 0, downsample_ds[next][level - 1], {});
			command_buffer.dispatch(groups(size.width), groups(size.height), view_count);
		}

		if (extrapolate)
		{
			command_buffer.bindPipeline(vk::PipelineBindPoint::eCompute, *flow_pass.pipeline);
			for (int level = levels - 1; level >= 0; --level)
			{
				compute_barrier(command_buffer);
				auto size = level_extent(extent, level);
				command_buffer.bindDescriptorSets(vk::PipelineBindPoint::eCompute, *flow_pass.layout, 0, flow_ds[next][level], {});
				command_buffer.pushConstants<flow_pc>(
				        *flow_pass.layout,
				        vk::ShaderStageFlagBits::eCompute,
				        0,
				        flow_pc{
				                .level = level,
				                .coarsest = int32_t(levels - 1),
				                .smoothness = smoothness,
				        });
				command_buffer.dispatch(groups(size.width), groups(size.height), view_count);
			}
		}

		compute_barrier(command_buffer, vk::PipelineStageFlagBits::eFragmentShader);

		current = next;
		previous = frames;
		have_previous = true;
	}
	// else the same frame again: nothing moved since it was submitted

	skip = not extrapolate;

	const vk::Viewport viewport{
	        .width = float(extent.width),
	        .height = float(extent.height),
	        .minDepth = 0,
	        .maxDepth = 1,
	};
	const vk::Rect2D scissor{.extent = extent};

	for (uint32_t view = 0; view < view_count; ++view)
	{
		command_buffer.beginRenderPass(
		        vk::RenderPassBeginInfo{
		                .renderPass = *motion_vector_renderpass,
		                .framebuffer = *motion_vector_framebuffers[motion_vector_index * view_count + view],
		                .renderArea = scissor,
		        },
		        vk::SubpassContents::eInline);
		command_buffer.setViewport(0, viewport);
		command_buffer.setScissor(0, scissor);
		command_buffer.bindPipeline(vk::PipelineBindPoint::eGraphics, *output_pass.pipeline);
		command_buffer.bindDescriptorSets(vk::PipelineBindPoint::eGraphics, *output_pass.layout, 0, output_ds, {});
		command_buffer.pushConstants<output_pc>(
		        *output_pass.layout,
		        vk::ShaderStageFlagBits::eVertex | vk::ShaderStageFlagBits::eFragment,
		        0,
		        output_pc{
		                .layer = int32_t(view),
		                .mode = extrapolate ? 1 : 0,
		                .threshold = match_threshold,
		        });
		command_buffer.draw(3, 1, 0, 0);
		command_buffer.endRenderPass();

		vk::ClearValue clear{vk::ClearDepthStencilValue{1.0, 0}};
		command_buffer.beginRenderPass(
		        vk::RenderPassBeginInfo{
		                .renderPass = *depth_renderpass,
		                .framebuffer = *depth_framebuffers[depth_index * view_count + view],
		                .renderArea = scissor,
		                .clearValueCount = 1,
		                .pClearValues = &clear,
		        },
		        vk::SubpassContents::eInline);
		command_buffer.endRenderPass();
	}

	last_submitted = display_time;
}

void stream_spacewarp::release()
{
	if (not acquired)
		return;
	motion_vectors.release();
	depth.release();
	acquired = false;
}

std::array<XrCompositionLayerSpaceWarpInfoFB, 2> stream_spacewarp::space_warp_views() const
{
	std::array<XrCompositionLayerSpaceWarpInfoFB, 2> views;
	for (uint32_t view = 0; view < view_count; ++view)
	{
		views[view] = {
		        .type = XR_TYPE_COMPOSITION_LAYER_SPACE_WARP_INFO_FB,
		        .layerFlags = skip ? XR_COMPOSITION_LAYER_SPACE_WARP_INFO_FRAME_SKIP_BIT_FB : XrCompositionLayerSpaceWarpInfoFlagsFB(0),
		        .motionVectorSubImage = {
		                .swapchain = motion_vectors,
		                .imageRect = {.extent = {int32_t(extent.width), int32_t(extent.height)}},
		                .imageArrayIndex = view,
		        },
		        .appSpaceDeltaPose = identity_pose,
		        .depthSubImage = {
		                .swapchain = depth,
		                .imageRect = {.extent = {int32_t(extent.width), int32_t(extent.height)}},
		                .imageArrayIndex = view,
		        },
		        .minDepth = 0,
		        .maxDepth = 1,
		        .nearZ = near_z,
		        .farZ = far_z,
		};
	}
	return views;
}

std::array<XrFrameSynthesisInfoEXT, 2> stream_spacewarp::frame_synthesis_views() const
{
	std::array<XrFrameSynthesisInfoEXT, 2> views;
	for (uint32_t view = 0; view < view_count; ++view)
	{
		views[view] = {
		        .type = XR_TYPE_FRAME_SYNTHESIS_INFO_EXT,
		        // the stream arrives at half the refresh rate
		        .layerFlags = XR_FRAME_SYNTHESIS_INFO_REQUEST_RELAXED_FRAME_INTERVAL_BIT_EXT,
		        .motionVectorSubImage = {
		                .swapchain = motion_vectors,
		                .imageRect = {.extent = {int32_t(extent.width), int32_t(extent.height)}},
		                .imageArrayIndex = view,
		        },
		        .motionVectorScale = {1, 1, 1, 0},
		        .motionVectorOffset = {0, 0, 0, 0},
		        .appSpaceDeltaPose = identity_pose,
		        .depthSubImage = {
		                .swapchain = depth,
		                .imageRect = {.extent = {int32_t(extent.width), int32_t(extent.height)}},
		                .imageArrayIndex = view,
		        },
		        .minDepth = 0,
		        .maxDepth = 1,
		        .nearZ = near_z,
		        .farZ = far_z,
		};
	}
	return views;
}
