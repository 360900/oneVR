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

#pragma once

#include "xr/hand_tracker.h"
#include <optional>
#include <openxr/openxr.h>

// The runtime tracks hands slower than the display, 50 Hz on a Quest 3 at 72 Hz, and gives the
// same sample again on the frames between, so a moving hand steps unevenly. This plays the samples
// back on the tracker's own steady clock, one sample interval late, and draws between them, which
// costs that interval in lag. A runtime that gives a new sample every frame passes through.
class hand_smoother
{
public:
	using joints = xr::hand_tracker::joint_array;

	// The joints to draw at display_time, nullopt while the hand is lost
	std::optional<joints> update(const std::optional<joints> & located, XrTime display_time);

private:
	// The last two samples and when they were taken
	std::optional<joints> from;
	std::optional<joints> to;
	XrTime from_time = 0;
	XrTime to_time = 0;

	XrTime shown_time = 0;
	XrTime last_seen = 0;
	XrTime interval = default_interval;
	XrTime frame_interval = default_interval;

	static constexpr XrTime default_interval = 20'000'000;
};
