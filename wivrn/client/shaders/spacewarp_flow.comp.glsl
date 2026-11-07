/*
 * oneVR: optical flow from the previous streamed frame to the current one, one pyramid level at
 * a time from the coarsest. Each pixel starts from the coarser level's flow or from the flow head
 * rotation alone explains, whichever matches better, and searches around it. Flow in pixels of
 * this level, where a point is now minus where it was
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
#extension GL_GOOGLE_include_directive : require

#include "spacewarp.glsl.inc"

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

layout(set = 0, binding = 0) uniform sampler2DArray current;
layout(set = 0, binding = 1) uniform sampler2DArray previous;
layout(set = 0, binding = 2) uniform sampler2DArray coarser;
layout(set = 0, binding = 3, rgba16f) uniform writeonly image2DArray flow;

layout(push_constant) uniform pc
{
	int level;
	int coarsest;
	// cost of straying from the head's motion, per image width
	float smoothness;
};

// a 3x3 patch spread over 5x5 pixels
const int taps = 9;
const int spread = 2;
// widest search, at the coarsest level
const int max_radius = 3;
const int max_candidates = (2 * max_radius + 1) * (2 * max_radius + 1);

ivec3 p;
vec2 size;
float current_taps[taps];

// x: how differently the patch looks there, y: that plus the penalties
vec2 cost(vec2 d, vec2 head)
{
	float sum = 0.0;
	int n = 0;
	for (int y = -1; y <= 1; ++y)
	{
		for (int x = -1; x <= 1; ++x)
		{
			vec2 q = (vec2(p.xy + spread * ivec2(x, y)) + 0.5 - d) / size;
			sum += abs(current_taps[n++] - textureLod(previous, vec3(q, p.z), float(level)).r);
		}
	}
	float match = sum / float(taps);

	vec2 q = (vec2(p.xy) + 0.5 - d) / size;
	float outside = any(lessThan(q, vec2(0.0))) || any(greaterThan(q, vec2(1.0))) ? 1.0 : 0.0;
	return vec2(match, match + outside + smoothness * length(d - head) / size.x);
}

float median9(float v[9])
{
	for (int i = 1; i < 9; ++i)
	{
		float x = v[i];
		int j = i - 1;
		while (j >= 0 && v[j] > x)
		{
			v[j + 1] = v[j];
			--j;
		}
		v[j + 1] = x;
	}
	return v[4];
}

vec2 from_coarser()
{
	ivec2 coarser_size = textureSize(coarser, level + 1).xy;
	ivec2 c = p.xy / 2;
	float xs[9];
	float ys[9];
	int n = 0;
	for (int y = -1; y <= 1; ++y)
	{
		for (int x = -1; x <= 1; ++x)
		{
			vec2 f = texelFetch(coarser, ivec3(clamp(c + ivec2(x, y), ivec2(0), coarser_size - 1), p.z), level + 1).xy;
			xs[n] = f.x;
			ys[n] = f.y;
			++n;
		}
	}
	return 2.0 * vec2(median9(xs), median9(ys));
}

void main()
{
	p = ivec3(gl_GlobalInvocationID);
	ivec2 isize = imageSize(flow).xy;
	if (any(greaterThanEqual(p.xy, isize)))
		return;
	size = vec2(isize);

	int n = 0;
	for (int y = -1; y <= 1; ++y)
		for (int x = -1; x <= 1; ++x)
			current_taps[n++] = texelFetch(current, ivec3(clamp(p.xy + spread * ivec2(x, y), ivec2(0), isize - 1), p.z), level).r;

	vec2 head = rotation_only_flow(vec2(p.xy) + 0.5, size, p.z);

	vec2 center = head;
	if (level < coarsest)
	{
		vec2 predicted = from_coarser();
		if (cost(predicted, head).y < cost(head, head).y)
			center = predicted;
	}

	int radius = level == coarsest ? max_radius : 1;
	float costs[max_candidates];
	float best = 1e9;
	float best_match = 1e9;
	ivec2 best_offset = ivec2(0);
	for (int y = -radius; y <= radius; ++y)
	{
		for (int x = -radius; x <= radius; ++x)
		{
			vec2 c = cost(center + vec2(x, y), head);
			costs[(y + max_radius) * (2 * max_radius + 1) + x + max_radius] = c.y;
			if (c.y < best)
			{
				best = c.y;
				best_match = c.x;
				best_offset = ivec2(x, y);
			}
		}
	}

	// a parabola through the best cost and its neighbours places the minimum between pixels
	vec2 sub = vec2(0.0);
	int i = (best_offset.y + max_radius) * (2 * max_radius + 1) + best_offset.x + max_radius;
	if (abs(best_offset.x) < radius)
	{
		float a = costs[i - 1];
		float c = costs[i + 1];
		float den = a - 2.0 * best + c;
		if (den > 1e-6)
			sub.x = clamp(0.5 * (a - c) / den, -0.5, 0.5);
	}
	if (abs(best_offset.y) < radius)
	{
		float a = costs[i - (2 * max_radius + 1)];
		float c = costs[i + (2 * max_radius + 1)];
		float den = a - 2.0 * best + c;
		if (den > 1e-6)
			sub.y = clamp(0.5 * (a - c) / den, -0.5, 0.5);
	}

	imageStore(flow, p, vec4(center + vec2(best_offset) + sub, best_match, 1.0));
}
