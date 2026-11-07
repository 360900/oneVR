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

#pragma once

#include "xr/hand_tracker.h"
#include <optional>
#include <openxr/openxr.h>

// How open a hand is for a fist: the cosine between the first and last segments of its most open
// finger, 1 straight and below 0 curled, the thumb counted by how far its tip is from the index and
// middle fingers. nullopt without valid joints. As WayVR's hand_input::fist_openness
std::optional<float> fist_openness(const xr::hand_tracker::joint_array & joints);

// Runtimes report a high grasp_ext value for a hand that points, gives a thumbs up or raises a
// finger, whose other fingers are curled, which games take for a grab. This passes the value only
// for a full fist held a moment; a grab that holds stays held without joints. As WayVR's GraspGate
class fist_gate
{
public:
	// The value to send for the runtime's grasp value at time
	float update(float value, std::optional<float> openness, XrTime time);

private:
	bool open = false;
	XrTime held_since = 0;
};
