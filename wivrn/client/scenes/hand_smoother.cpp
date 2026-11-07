/*
 * oneVR: hands that glide between the tracker's samples
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

#include "hand_smoother.h"

#include <algorithm>
#include <cmath>
#include <glm/geometric.hpp>
#include <glm/gtc/quaternion.hpp>
#include <glm/vec3.hpp>

namespace
{
// Gaps outside this are a lost or paused hand, not the tracker's rate
constexpr XrTime min_interval = 5'000'000;
constexpr XrTime max_interval = 50'000'000;
// Share of a new gap in the measured intervals. Samples come one or two frames apart, so the
// tracker's interval is only right over many of them.
constexpr float frame_weight = 0.2;
constexpr float sample_weight = 0.02;
// A wrist that moved this far between samples was found again elsewhere and isn't glided to, in m
constexpr float jump = 0.2;

constexpr XrSpaceLocationFlags pose_valid = XR_SPACE_LOCATION_POSITION_VALID_BIT | XR_SPACE_LOCATION_ORIENTATION_VALID_BIT;

glm::vec3 position(const XrHandJointLocationEXT & joint)
{
	return {joint.pose.position.x, joint.pose.position.y, joint.pose.position.z};
}

glm::quat orientation(const XrHandJointLocationEXT & joint)
{
	return {joint.pose.orientation.w, joint.pose.orientation.x, joint.pose.orientation.y, joint.pose.orientation.z};
}

bool same_sample(const hand_smoother::joints & a, const hand_smoother::joints & b)
{
	return std::ranges::equal(a, b, [](const auto & x, const auto & y) {
		const auto & p = x.first.pose;
		const auto & q = y.first.pose;
		return x.first.locationFlags == y.first.locationFlags and
		       p.position.x == q.position.x and p.position.y == q.position.y and p.position.z == q.position.z and
		       p.orientation.x == q.orientation.x and p.orientation.y == q.orientation.y and
		       p.orientation.z == q.orientation.z and p.orientation.w == q.orientation.w;
	});
}

hand_smoother::joints mix(const hand_smoother::joints & from, const hand_smoother::joints & to, float t)
{
	auto out = to;
	for (size_t i = 0; i < out.size(); i++)
	{
		const auto & a = from[i].first;
		const auto & b = to[i].first;
		if ((a.locationFlags & pose_valid) != pose_valid or (b.locationFlags & pose_valid) != pose_valid)
			continue;

		glm::vec3 p = glm::mix(position(a), position(b), t);
		glm::quat q = glm::normalize(glm::slerp(orientation(a), orientation(b), t));
		auto & joint = out[i].first;
		joint.pose.position = {p.x, p.y, p.z};
		joint.pose.orientation = {q.x, q.y, q.z, q.w};
		joint.radius = std::lerp(a.radius, b.radius, t);
	}
	return out;
}
} // namespace

std::optional<hand_smoother::joints> hand_smoother::update(const std::optional<joints> & located, XrTime display_time)
{
	if (not located)
	{
		*this = {};
		return std::nullopt;
	}

	if (not to or display_time <= shown_time)
	{
		from = to = located;
		from_time = to_time = shown_time = display_time;
		return located;
	}

	XrTime frame = display_time - shown_time;
	shown_time = display_time;
	if (frame >= min_interval and frame <= max_interval)
		frame_interval += XrTime(frame_weight * float(frame - frame_interval));

	if (not same_sample(*located, *to))
	{
		// The sample was taken after the last frame. On a steady tracker it is one interval after
		// the one before, which keeps the times even where the frames that show them are not.
		XrTime earliest = display_time - frame_interval;
		XrTime expected = to_time + interval;
		XrTime taken = std::clamp(expected, earliest, display_time);
		XrTime gap = taken - to_time;
		XrTime seen_gap = display_time - last_seen;
		if (seen_gap >= min_interval and seen_gap <= max_interval)
			interval += XrTime(sample_weight * float(seen_gap - interval));
		last_seen = display_time;

		const auto & wrist_to = (*to)[XR_HAND_JOINT_WRIST_EXT].first;
		const auto & wrist_located = (*located)[XR_HAND_JOINT_WRIST_EXT].first;
		bool found_again = gap > max_interval or glm::distance(position(wrist_to), position(wrist_located)) > jump;
		from = found_again ? located : to;
		from_time = found_again ? taken - interval : to_time;
		to = located;
		to_time = taken;
	}

	// A tracker as fast as the display needs no help
	if (interval <= frame_interval + frame_interval / 8)
		return to;

	// Shown one interval late, so the next sample is mostly there in time. Until the frame it
	// should have come by, the hand goes on in the same direction, then back to the last sample,
	// where a hand the tracker lost or that stopped stays.
	XrTime shown_at = display_time - interval;
	XrTime span = std::max<XrTime>(to_time - from_time, 1);
	float t = float(shown_at - from_time) / float(span);
	float overshoot = float(frame_interval) / float(span);
	if (t > 1)
		t = std::max(1.f, 1 + overshoot - std::abs(t - 1 - overshoot));
	if (t >= 1 and t <= 1)
		return to;
	return mix(*from, *to, std::max(t, 0.f));
}
