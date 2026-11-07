/*
 * oneVR: a hand drawn like a sketch: a flat dark tint inside, held by a flat light outline drawn
 * with hand_outline_skinned.vert, each its material's base color
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

#include "common.glsl.inc"

// Fragment output
layout(location = 0) out vec4 out_color;

void main()
{
    // premultiplied, the eye buffer is blended over passthrough and the panels
    out_color = vec4(material.base_color_factor.rgb * material.base_color_factor.a, material.base_color_factor.a);
}
