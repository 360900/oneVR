/*
 * oneVR: the luma at the base of a frame's space warp pyramid, from the frame drawn at twice
 * its size: one bilinear sample averages the 2x2 pixels under each output pixel
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
#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) uniform sampler2DArray color;
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2DArray luma;

void main()
{
	ivec3 p = ivec3(gl_GlobalInvocationID);
	ivec2 size = imageSize(luma).xy;
	if (any(greaterThanEqual(p.xy, size)))
		return;

	vec2 uv = (vec2(p.xy) + 0.5) / vec2(size);
	vec3 rgb = textureLod(color, vec3(uv, p.z), 0.0).rgb;
	// closer to perceived lightness, so dark areas still match
	float y = sqrt(max(dot(rgb, vec3(0.2126, 0.7152, 0.0722)), 0.0));
	imageStore(luma, p, vec4(y, 0.0, 0.0, 1.0));
}
