/*
 * oneVR: a hand's grasp counts only for a fist, not a thumbs up or a raised finger
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

#include "hand_fist.h"

#include <algorithm>
#include <glm/geometric.hpp>
#include <glm/vec3.hpp>

namespace
{
// The same values as WayVR's hand_input.rs and the hand profile in openxr_actions.json5
constexpr float press = 0.95;
constexpr float release = 0.8;
constexpr XrTime hold = 120'000'000;
// fist_openness at or below this may start a grab: every finger bent 90 degrees or more
constexpr float fist_closed = 0.0;
// fist_openness above this ends a grab: a finger out by about 50 degrees
constexpr float fist_open = 0.6;
// the thumb's tip this near the index or middle finger, in hand lengths, is in the fist, and
// this far out of it, as in a thumbs up
constexpr float thumb_in = 0.5;
constexpr float thumb_out = 0.7;
} // namespace

std::optional<float> fist_openness(const xr::hand_tracker::joint_array & joints)
{
	for (const auto & joint: joints)
		if (not(joint.first.locationFlags & XR_SPACE_LOCATION_POSITION_VALID_BIT))
			return std::nullopt;

	auto position = [&](XrHandJointEXT joint) {
		const auto & p = joints[joint].first.pose.position;
		return glm::vec3{p.x, p.y, p.z};
	};
	auto bend = [&](XrHandJointEXT proximal) -> std::optional<float> {
		// proximal, intermediate, distal and tip follow each other
		glm::vec3 first = position(XrHandJointEXT(proximal + 1)) - position(proximal);
		glm::vec3 last = position(XrHandJointEXT(proximal + 3)) - position(XrHandJointEXT(proximal + 2));
		if (glm::length(first) <= 0 or glm::length(last) <= 0)
			return std::nullopt;
		return glm::dot(glm::normalize(first), glm::normalize(last));
	};

	float hand_length = glm::distance(position(XR_HAND_JOINT_WRIST_EXT), position(XR_HAND_JOINT_MIDDLE_PROXIMAL_EXT));
	if (hand_length <= 0)
		return std::nullopt;
	glm::vec3 thumb_tip = position(XR_HAND_JOINT_THUMB_TIP_EXT);
	float thumb_gap = std::min(glm::distance(thumb_tip, position(XR_HAND_JOINT_INDEX_INTERMEDIATE_EXT)),
	                           glm::distance(thumb_tip, position(XR_HAND_JOINT_MIDDLE_INTERMEDIATE_EXT))) /
	                  hand_length;
	// on the fingers' scale: in the fist at thumb_in, out of it at thumb_out
	float openness = fist_closed + (thumb_gap - thumb_in) / (thumb_out - thumb_in) * (fist_open - fist_closed);

	for (auto proximal: {XR_HAND_JOINT_INDEX_PROXIMAL_EXT, XR_HAND_JOINT_MIDDLE_PROXIMAL_EXT, XR_HAND_JOINT_RING_PROXIMAL_EXT, XR_HAND_JOINT_LITTLE_PROXIMAL_EXT})
	{
		auto finger = bend(proximal);
		if (not finger)
			return std::nullopt;
		openness = std::max(openness, *finger);
	}
	return openness;
}

float fist_gate::update(float value, std::optional<float> openness, XrTime time)
{
	if (open)
	{
		open = value >= release and not(openness and *openness > fist_open);
		held_since = 0;
	}
	else if (value >= press and openness and *openness <= fist_closed)
	{
		if (not held_since)
			held_since = time;
		open = time - held_since >= hold;
	}
	else
		held_since = 0;

	return open ? value : 0;
}
