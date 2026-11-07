/*
 * oneVR: the motion vectors the runtime extrapolates the stream with: the finest optical flow,
 * median filtered, in NDC per submitted frame. Where the flow found no good match, only the head
 * moved, which the runtime's own reprojection handles
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

layout(push_constant) uniform pc
{
	int layer;
	// 0: nothing moved, the frame was shown before; 1: the flow
	int mode;
	// the worst match the flow is trusted with
	float threshold;
};

#ifdef VERT_SHADER

void main()
{
	vec2 position = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
	gl_Position = vec4(position * 2.0 - 1.0, 0.0, 1.0);
}

#endif

#ifdef FRAG_SHADER

#include "spacewarp.glsl.inc"

layout(set = 0, binding = 0) uniform sampler2DArray flow;

layout(location = 0) out vec4 motion_vector;

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

void main()
{
	if (mode == 0)
	{
		motion_vector = vec4(0.0);
		return;
	}

	ivec2 p = ivec2(gl_FragCoord.xy);
	ivec2 isize = textureSize(flow, 0).xy;
	vec2 size = vec2(isize);

	float xs[9];
	float ys[9];
	int n = 0;
	for (int y = -1; y <= 1; ++y)
	{
		for (int x = -1; x <= 1; ++x)
		{
			vec2 f = texelFetch(flow, ivec3(clamp(p + ivec2(x, y), ivec2(0), isize - 1), layer), 0).xy;
			xs[n] = f.x;
			ys[n] = f.y;
			++n;
		}
	}
	vec2 d = vec2(median9(xs), median9(ys));

	float match = texelFetch(flow, ivec3(p, layer), 0).z;
	if (match > threshold)
		d = rotation_only_flow(vec2(p) + 0.5, size, layer);

	motion_vector = vec4(2.0 * d / size * views[layer].misc.x, 0.0, 0.0);
}

#endif
