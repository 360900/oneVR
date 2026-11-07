/*
 * oneVR: the lobby's hands as oneDesign dark glass, a faint dark fill held by a crisp outline
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
#extension GL_EXT_multiview : require

#include "common.glsl.inc"

// Fragment output
layout(location = 0) out vec4 out_color;

// Where the outline starts and is full, in 1 - cos of the angle to the eye
const float outline_start = 0.4;
const float outline_full = 0.65;
// The glow inside the outline that gives the glass its thickness, oneDesign's glass.edge
const float glow = 0.6;
const float glow_power = 3.0;
// The rim lit from above, oneDesign's glass.specular, and how much of the outline it takes below
const float specular = 0.3;
const float unlit = 0.35;

void main()
{
    vec3 n = normalize(normal);
    vec3 view_dir = normalize(-frag_pos.xyz);
    vec3 up = normalize(mat3(scene.view[gl_ViewIndex]) * vec3(0, 1, 0));

    float turned = 1 - max(dot(n, view_dir), 0);
    float lit = 0.5 + 0.5 * dot(n, up);

    // fill: base color, its alpha the share of the room it darkens
    vec3 fill = material.base_color_factor.rgb;
    float fill_alpha = material.base_color_factor.a;

    // outline: emissive, its alpha the opacity of the outline's top
    float outline = smoothstep(outline_start, outline_full, turned);
    float edge = outline + glow * pow(turned, glow_power) * (1 - outline);
    edge *= mix(1 - unlit, 1, lit) + specular * lit * outline;
    float edge_alpha = clamp(material.base_emissive_factor.a * edge, 0, 1);

    // premultiplied: the edge over the fill, blended over passthrough and the panels
    vec3 color = fill * fill_alpha * (1 - edge_alpha) + material.base_emissive_factor.rgb * edge_alpha;
    out_color = vec4(color, fill_alpha + edge_alpha * (1 - fill_alpha));
}
