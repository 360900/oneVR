/*
 * oneVR: the next level of a space warp luma pyramid, each pixel the mean of the 2x2 under it
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

layout(set = 0, binding = 0, rgba16f) uniform readonly image2DArray src;
layout(set = 0, binding = 1, rgba16f) uniform writeonly image2DArray dst;

void main()
{
	ivec3 p = ivec3(gl_GlobalInvocationID);
	if (any(greaterThanEqual(p.xy, imageSize(dst).xy)))
		return;

	ivec2 last = imageSize(src).xy - 1;
	ivec2 s = 2 * p.xy;
	float sum = imageLoad(src, ivec3(min(s, last), p.z)).r +
	            imageLoad(src, ivec3(min(s + ivec2(1, 0), last), p.z)).r +
	            imageLoad(src, ivec3(min(s + ivec2(0, 1), last), p.z)).r +
	            imageLoad(src, ivec3(min(s + ivec2(1, 1), last), p.z)).r;
	imageStore(dst, p, vec4(0.25 * sum, 0.0, 0.0, 1.0));
}
