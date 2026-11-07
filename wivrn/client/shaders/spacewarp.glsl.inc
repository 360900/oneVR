/*
 * oneVR: what the space warp passes share. Images span a frame's whole field of view, with uv
 * in [0, 1] and v going down like Vulkan's NDC y
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

struct spacewarp_view
{
	// the current frame's view space to the previous frame's, rotation only
	mat4 rotation;
	// tangents of the field of view: left, right, up, down
	vec4 tan_current;
	vec4 tan_previous;
	// x: the submitted frame interval over the streamed one
	vec4 misc;
};

layout(std140, set = 0, binding = 4) uniform spacewarp_params
{
	spacewarp_view views[2];
};

// Where the previous frame showed what the current one shows at uv, if only the head turned.
// Far outside [0, 1] when it was behind the previous view
vec2 rotation_only_previous_uv(vec2 uv, int layer)
{
	vec4 t = views[layer].tan_current;
	vec3 dir = vec3(mix(t.x, t.y, uv.x), mix(t.z, t.w, uv.y), -1.0);
	vec3 p = mat3(views[layer].rotation) * dir;
	if (p.z > -1e-4)
		return vec2(-100.0);

	vec2 tan_p = p.xy / -p.z;
	vec4 t2 = views[layer].tan_previous;
	return vec2((tan_p.x - t2.x) / (t2.y - t2.x), (t2.z - tan_p.y) / (t2.z - t2.w));
}

// The head's motion in pixels of an image of this size: where a point is now minus where it was
vec2 rotation_only_flow(vec2 pixel, vec2 size, int layer)
{
	vec2 uv = pixel / size;
	vec2 previous = rotation_only_previous_uv(uv, layer);
	if (any(lessThan(previous, vec2(-10.0))))
		return vec2(0.0);
	return (uv - previous) * size;
}
