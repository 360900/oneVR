#version 450
#extension GL_GOOGLE_include_directive : enable

precision highp float;

layout(location = 0) in vec4 in_color;
layout(location = 1) in vec4 in_color2;
layout(location = 2) in vec2 in_uv;
layout(location = 3) in vec4 in_border_color;
layout(location = 4) in float in_border_size; // in units
layout(location = 5) in float in_radius;      // in units
layout(location = 6) in vec2 in_rect_size;
layout(location = 7) flat in uint in_mode;
layout(location = 0) out vec4 out_color;

#define UNIFORM_PARAMS_SET 0
#include "uniform.glsl"
#include "srgb.glsl"

// Abramowitz-Stegun style approximation
float erf_approx(float x) {
  float s = sign(x);
  float a = abs(x);
  float t = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
  t *= t;
  return s - s / (t * t);
}

// mode 4: soft box shadow. The quad is the casting box grown by the blur
// extent (in_border_size); in_radius is the casting box corner radius.
void shadow_main() {
  vec2 center = in_rect_size / 2.0;
  vec2 coords = in_uv * in_rect_size;
  float blur = in_border_size;
  vec2 half_box = max(center - vec2(blur), vec2(0.0));
  float radius = min(in_radius, min(half_box.x, half_box.y));
  vec2 q = abs(coords - center) - (half_box - vec2(radius));
  float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
  float sigma = max(blur * 0.5, 0.5);
  float a = (0.5 - 0.5 * erf_approx(dist / (sigma * 1.41421356))) * in_color.a;
  // match sRGB-space (CSS) shadow darkening on light backgrounds when
  // blending happens in linear space
  a = 1.0 - pow(1.0 - a, 2.2);
  out_color = vec4(to_srgb(in_color.rgb), a);
}

void main() {
  if (in_mode == 4u) {
    shadow_main();
    return;
  }

  float rect_aspect = in_rect_size.x / in_rect_size.y;

  vec2 center = in_rect_size / 2.0;
  vec2 coords = in_uv * in_rect_size;

  float radius = in_radius;

  vec2 sdf_rect_dim = center - vec2(radius);
  float sdf = length(max(abs(coords - center), sdf_rect_dim) - sdf_rect_dim) -
              in_radius;

  vec4 color =
      mix(in_color, in_color2, min(length((in_uv - vec2(0.5)) * 2.0), 1.0));

  float pixel_size = 1.0 / uniforms.pixel_scale;

  if (in_border_size < in_radius) {
    // rounded border
    float f = in_border_size > 0.0 ? smoothstep(in_border_size + pixel_size,
                                                in_border_size, -sdf) *
                                         in_border_color.a
                                   : 0.0;
    out_color = mix(color, in_border_color, f);
  } else {
    // square border
    vec2 a = abs(coords - center);
    float aa = center.x - in_border_size;
    float bb = center.y - in_border_size;
    out_color = (a.x > aa || a.y > bb) ? in_border_color : color;
  }

  if (in_radius > 0.0) {
    // rounding cutout alpha
    out_color.a *= 1.0 - smoothstep(-pixel_size, 0.0, sdf);
  }
  out_color.rgb = to_srgb(out_color.rgb);
}
