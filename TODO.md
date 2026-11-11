# oneVR TODO

Reconciled against CHANGES.md on 2026-10-31. Items already implemented per CHANGES
are removed. The 2026-10-30 branch work is merged to main (f598d729) and pushed; CI is
building the APK. Open upstream issues and PRs of WiVRn, wayvr and xrizer were checked
against main on 2026-11-11 (see "Open: from upstream issues and PRs"). xrizer items live
in xrizer/TODO.md (edited in xrizer-edge).

## Verify on the Quest (shipped this session, not yet confirmed on device)

- Blocker 1, screens crash: the oneVR DMA-buf copy is now opt-in
  (`copy_captured_frames: true` in `~/.config/wayvr/onevr.json5`, only on GPUs with a
  second graphics queue), default off = upstream in-place sampling. Confirm screens show
  without the Xid 69 device-lost crash.
- Blocker 2, Steam runtime: `~/.config/environment.d/onevr.conf` is written and the var is
  active. Confirm a Steam VR game opens in the headset, not on the monitor, and that
  `onevr doctor`'s Steam game sandbox row, `onevr steam` and PC > Server's Steam launch
  options agree with it (all three now read the running Steam's environment).
- Hand grab (#18): grasp thresholds raised to press 0.9. Confirm a pointing hand no longer
  grabs and the dashboard does not reopen on its own; tune if still off. A double fist now
  counts only while the hand points at nothing and holds nothing (a quick regrab toggled
  the dashboard): confirm regrabbing a screen toggles nothing and both double fists work.
- Hand gestures (#3): a thumb+middle pinch right clicks, thumb+ring middle clicks (pointer
  turns orange/purple); a held fist pushes/pulls the panel by reach; a second fist on the
  same panel resizes by spread; grab tips show hand pictures above the fist. Confirm the
  pinches don't fire in a fist or an index pinch, and tune `FINGER_PINCH_*` (hand_input.rs),
  `DEPTH_POWER` (hand_grab.rs) and `GRAB_HELP_ABOVE_HAND` (backend/input.rs) if off.
- Hand tracking tutorial page and hand icons: the tutorial, shown on a first launch, has a
  Hand tracking page after Laser Colors, and the grab tips use new Phosphor-based icons. Not
  seen rendered yet: check both in VR (set `tutorial_graduated: false` in
  `~/.config/wayvr/conf.d/zz-saved-config.json5` to see the tutorial again). oneVR's Home has
  no button to reopen the tutorial, unlike upstream's.
- Cover options gear (#8): now overlaid in the cover's top-right corner; games not on Auto
  show a short Touch/Index badge with an icon beside it. Confirm both stay inside the cover
  and the badge reads on bright covers.
- Dropdown dim backdrop (#16): off in VR, and the click catcher behind menus is now
  transparent (it was opaque black). Confirm nothing darkens the panels behind dropdowns
  and menus.
- Lobby close button (#25): reworded to "Close the app"; needs the new APK from CI.
- Lobby bottom bar (#24): its rounded ends now get the bar's shadows and shine instead of
  a lighter plain fill; needs the new APK.
- Library launch: it toasts, plays the launch sound and badges the app; a second click
  within 10 s is ignored; a start the server refuses toasts why, drops the badge and can
  be clicked again at once.
- Game titles: Now playing, Running and Stop's confirm show a Library-started OpenVR game's
  title, not hlvr, also when the dashboard was closed right after the click and reopened
  over 3 minutes later. A game launched from All Steam games while a Library start is
  pending shows its own name.
- VR keyboard: clicking the Library search shows it, and after the clear button it keeps
  typing into the box, not the PC's window. Hidden from the watch, it stays hidden on a
  press elsewhere (between covers, on a label); a press on the box shows it.
- PC > Diagnostics > Fix in VR (wayvr runs under onevr-server.service): the admin password
  prompt shows on the PC's screen and the row updates after it; cancelling the prompt
  toasts "Couldn't apply the fix: the admin password prompt was closed".
- Home's Codec and Resolution tiles show what the session streams (e.g. HEVC 10-bit with
  the codec on Automatic) and keep the running values after a restart-only change until
  Restart session.
- Pairing from the desktop window during a session: one confirm, Getting a PIN..., then the
  PIN. With oneVR stopped, the tray's Pair headset starts it first; Pair headset again while
  a PIN shows keeps that PIN.
- Stats pause: stream with the desktop window on PC > Performance, then open the headset's
  system menu or take it off for 5 s. The graphs stay and go on when you return; the stage
  values, Home's tiles and the VR overlay show "-" meanwhile. Disconnecting still shows
  "Statistics appear while a headset streams."
- Upstream issue fixes from 2026-11-11, in VR: a notification on the watch lies on its
  face, with a controller and with a tracked hand (`notification_topics` set to Watch,
  wayvr #389); after standing up or sitting down, opening the dashboard or switching sets
  opens panels at the new height (#316); double clicks on a screen and in an app in WayVR
  with the click freeze on (#720), and an Electron app such as Spotify in WayVR clicks
  instead of dragging (#657); a custom command bound to a face button runs (#578); with
  "Mouse move on trigger touch" on, an app in WayVR follows the pointer only on trigger
  touch (#682); with input captured (Super+Z), the PC's mouse keeps working in an app in
  WayVR after the pointer leaves it (#721); a screen set back to its default stereo or
  mouse transform stays so after a restart (PR #662); a body tracking failure leaves the
  stream running (WiVRn #1135).
- Also: Library sections and no duplicate covers (#6/#7), no browser entry (#9), codec
  above foveation (#11), body-tracker toggles wrap (#22, shown only with Body tracking on),
  passthrough default off (#13), advanced-setting descriptions (#21).

## Open: from the 2026-10-30 test session

Headset interaction (do with the device):
- Watch wrist alignment (#4): with a tracked hand the watch now sits on the wrist joint
  (back of the forearm, `WRIST_WATCH_POS` in overlays/watch.rs); confirm on the Quest and tune.
  With controllers it still follows the aim pose; a GRIP pose (`/input/grip/pose`) for the
  watch there would need `OpenXrInputProfile.grip`, a `Pointer.grip_pose` and a grip-following
  positioning. Tune WATCH_POS/WATCH_ROT on the Quest.
- Choppy pointers/rays (#5): pose is lerped from wayvr's own loop fps; choppiness points at
  a low render/update rate (power governor or capture stalls). Two headset causes are fixed:
  streams ran at the lobby's power savings clocks, and after a stall a stream kept the
  lobby's lowest refresh rate. Recheck, then profile the loop fps if still choppy.
- Hand poke of panels (#19): WayVR panels take ray + pinch, not a finger poke like Horizon
  OS / the WiVRn lobby. Needs a poke interaction (poke_ext/pose, near-field touch against
  panel planes). Larger feature.

Server and dashboard:
- Auto audio switch (#14): WiVRn makes a PipeWire sink "wivrn.sink" (desc "oneVR",
  Audio/Sink) per session in server/audio/audio_pipewire.cpp (wivrn_session.cpp:106). Switch
  the PC default to it on connect and restore on disconnect via the "default" metadata
  (default.configured.audio.sink) from the audio pipewire loop, saving/restoring the prior
  value. Gate behind a config option (default on). Verify on the device; don't shell out to
  pactl/wpctl in the shipped server.
- Game-exited-without-connecting message (blocker-2 follow-up): detect in the server when a
  launched app exits while never opening an XR session, and send a new onevr_packets message
  so the headset shows "the game closed without starting in VR; check the Steam launch
  option / re-login". Needs app tracking in server/main.cpp (forked_children exit) crossed
  with whether an XR session opened, plus a headset-side notice.
- Toasts under the dashboard (#10): place server/feedback toasts beneath the dashboard
  while it is shown, instead of their current spot.
- Autostart list is empty (#12): Settings > Apps to run on startup shows nothing. List the
  Apps tab's entries with a toggle (grey/green outline or mono/colored icon).
- Posture dropdown placement (#17): put standing/seated next to the recenter button with no
  "Posture" label. The recenter button is on the watch (wayvr), so this is a watch control
  wired to the server's headset config, a cross-component change.
- See-through slider live (#20): the passthrough-fade slider should apply while dragging,
  not only on release.
- Lobby layout (#23): drop the separator between the computer list and "Add server"; put
  the list under the heading "Pick a PC running the oneVR server to stream from."
- Real remote stop (optional, from #25): a headset action that actually stops the PC
  session (StopSession/StopServer over the connection), beyond the current close-the-app.

## Open: project roadmap (only what is not already in CHANGES)

Stability, verify on this build:
- WiVRn #873: CachyOS RTX 4050 hybrid laptop, periodic flashes of an old frame. Recheck
  after today's build; may be resolved by the blocker-1 fix or the #1153 Monado patch.

Known bugs and limits:
- Monado onevr-0004 at session end: for apps without a timeline semaphore
  (XR_KHR_vulkan_enable, so Proton games), swapchains destroyed after the last frame are
  freed at once, and work queued after the last xrEndFrame is not waited for. A fence per
  swapchain on each release barrier would cover both.
- A headset the running session refuses on reconnect (codec, bit depth, audio or headset
  features changed) gets a new session, and the game closes with only a server log line.
  The server could send the reason as a HeadsetMessage once the new dashboard is linked.
- BGR888 screen capture: wlroots with the GLES2 renderer on NVIDIA (seen on 0.20 with an
  RTX 5080) offers only BGR888 shared memory, which neither wlr_screencopy.rs nor
  ext_image_copy_capture.rs takes, so screens and virtual monitors stay blank on
  "ScreenCopy CPU", and on GPU capture once it falls back to shared memory. Needs
  Bgr888/Rgb888 in both (image copy wants a stride that is a multiple of 3) and each row
  expanded to Xbgr8888/Xrgb8888 before upload, as fourcc_to_vk has no 24-bit format.
- KWin before 6.8 offers its screencast to wayvr only if io.github.onevr.wayvr.desktop is
  found. When it isn't, only VR says so; `onevr doctor` (and so PC > Diagnostics) could
  check the entry and its Exec, and fix it by linking it into ~/.local/share/applications.
- Virtual monitors: Home lists one still being restored like a working one; only the toast
  at start says it is retried. After removing a lower-numbered one, the rest are renamed at
  the next start ([oneVR-1, oneVR-3] come back as oneVR-1 and oneVR-2), and the renamed one
  loses its placement and settings. Saving the names in onevr.json5 would fix it.
  Compositor detection still trusts another login's Hyprland: a launcher can hand Hyprland
  its Wayland socket, so a pid check can't rule it out.
- A Library page restored after a restart can toast "The oneVR server is not running." when
  its first request beats the link's first answer. new() could skip the request while the
  link is down, as update() asks once it is up.
- CI debug APKs get a new signing key each run, so one can't update another, until the repo
  secret APK_DEBUG_KEYSTORE_BASE64 holds a debug keystore in base64. With the local one
  (`base64 -w0 ~/.android/debug.keystore`), local and CI debug builds update each other.
  After setting it, uninstall com.weldiva.onevr.debug once.

Features not yet built:
- Starting bitrate from a UDP burst during the handshake.
- Black edges on fast head turns: render a margin past the FOV for the headset's
  reprojection (distinct from the FOV-shrink sliders already shipped).
- Game compatibility database: JSON keyed by Steam app id with launch options, xrizer
  settings and controller profile.
- Snapdragon GSR upscaling on the headset after decoding (BSD licensed).
- Remote play: UPnP with miniupnpc, connect by address on the headset, a doctor check.
- Video player: libmpv into a WayVR overlay, with SBS, 180 and 360 projection.
- Cinema environments and a hand or desk passthrough cutout.
- WiVRn #540: continuous adaptive bitrate, on top of the UDP-burst starting bitrate. The
  maintainer found bandwidth estimates unreliable.

Research first:
- Oculus SDK games: ReviveXR in the Proton prefix over wineopenxr. Try one game.
- OpenXR 1.1 and quad views: track upstream Monado.
- Other headsets: Pico needs testing; others need hardware or testers.

## Open: from upstream issues and PRs (checked 2026-11-11)

Open issues not listed here or above are fixed in oneVR, don't apply to it (WiVRn's Qt
dashboard, flatpak, the in-stream window), are translations that come with merges, or
belong to Monado, a driver or a headset runtime.

Bugs, server:
- VAAPI ignores a bitrate change during a session (WiVRn #690): video_encoder_ffmpeg.cpp
  sets `bit_rate` on the open context, which VAAPI reads only when it opens, while the
  Headset page sends the bitrate live. Rebuild the encoder with an IDR on a pending bitrate
  or frame rate, or make the bitrate restart-only on VAAPI. Hits every Intel GPU.
- Index hand root not clamped (WiVRn #1160): Monado patch 0010 limits extrapolation in
  `Device::get_pose` but not for the root in `ControllerDevice::get_hand_tracking`. Extend
  it in an onevr patch; verify on an Index.

Bugs, headset:
- Pico 4 left Menu does nothing (WiVRn #1210): stream.cpp maps the left `menu/click` and
  `system/click` to one id, and the never-pressed system click overwrites it, so WayVR's
  show/hide on it fails. WiVRn PR #1213 fixes it (protocol change: server and APK
  together); take it once merged, cherry-pick if it stalls.
- Battery saver holds a Quest at 72 Hz while the server renders the asked rate (WiVRn
  #1200): the client never reads `get_current_refresh_rate()` back. Read it after each
  request and report a rate the runtime kept. Verify on a Quest in battery saver.

Small features:
- A description under Body tracking (WiVRn #713): Quest and Pico always offer it, so
  SlimeVR users who turn it on get phantom trackers.
- Battery sent on Android's change events instead of every 30 s (WiVRn #751).
- NVENC preset, tuning, rate control, VBV and AQ from the encoder's `options`, as VAAPI
  and Vulkan already read theirs (WiVRn #1093).
- `onevr-ui --tray`: start in the tray without the window, with an optional login
  autostart (WiVRn #1089).
- A saturation slider beside CAS sharpening (WiVRn #711).
- Search and sort in the Games tab like the Library, or drop the tab for Library's Add apps
  (wayvr #551, #556).
- wayvrctl: query the current set and screen visibility (wayvr #712), reload panels.yaml
  (#714).

Larger features, only if wanted:
- The headset's play area to apps: the headset sends its stage bounds and a Monado patch
  answers `xrGetReferenceSpaceBoundsRect` (WiVRn #217).
- Saved settings profiles beside the graphics presets (WiVRn #738), joystick navigation in
  the lobby (#758), headset brightness (#604; first check Android's screenBrightness on a
  Quest), the headset view in a desktop window (#404).
- wayvr requests: drag and rotate the space on one binding (#646), relative rotate (#718),
  several inputs per action in the bindings view (#717), stereo per set (#659), saved
  mirrors (#164), window snapping (#315), vertical curvature (#165), resize with keyboard
  and mouse (#724), a launcher on the watch (#722), a TKL keyboard (#625), capture
  keybinds (#612), Whisper push-to-talk (#642), a toast line limit (#335), long-press
  bindings (#229), press-and-release menus (#358), OSC overlay poses and keyboard (#264,
  #120), locate a device from the watch (#327), OVR Advanced Settings style playspace
  toggles (#560). Physical keyboard typing (#655) and touch-style scrolling (#568) come
  with hand poke (#19). Suggest declining KMS capture (#705) and gamepad control (#348).

Needs hardware to reproduce or verify:
- AMD Vulkan encode: GPU timeouts and device lost in long sessions on RDNA3/4 (WiVRn #841,
  #870); compare RADV quality level 0 with oneVR's 1.
- Intel Arc pink or grey eye (WiVRn #813): check Auto picks HEVC over AV1 on VAAPI. Quest 3
  purple eye after 20 to 30 min of AV1 (#867): check the AV1 bitrate cap.
- NVIDIA: the headset goes to the lobby while the game still renders (WiVRn #987); stutter
  on a 4 GB card (#954, likely fixed with the queue lock fix).
- Quest: fast controller tracking (WiVRn #234), Quest Pro face and eye tracking to OSC
  (#560), Half-Life: Alyx load screens (#888), RUMBLE's controllers at the feet with
  `XR_META_touch_controller_plus` (#826, likely a Monado binding patch), Close all apps
  with VRChat and No Man's Sky running (#1084).
- Pico 4: invisible rays (WiVRn #1005), Pavlov walks inverted (#1169). Pico 4 Ultra: no
  index pinch click from hands (#1196); fall back to an index pinch from the joints.
- Steam Frame: double vision on hands and rays (WiVRn #1140), image jitter (#1173).
- Games under xrizer-edge: Overload (WiVRn #598), Dagon (#829), Elite Dangerous losing focus
  after a session recreate (#1029), Ragnarock Demo's buttons (xrizer #465, free demo; note
  the result in xrizer-edge's CHANGES).
- Desktops: rotated output on Hyprland (wayvr #726), Firefox tab bar scrolling on KDE
  (#227; try sending `REL_WHEEL` with the high resolution wheel), a Qt flatpak app in
  WayVR (#675).

Take from upstream:
- WiVRn PR #1213: the Pico 4 menu fix above.
- WiVRn PR #1208: Steam Frame bumper clicks and controller batteries. It removes
  `get_battery_status()`, which stream_onevr.cpp calls, so port that call with it.
- WiVRn PR #1194: no Wi-Fi scans during a Frame stream; needs a D-Bus policy and a setup
  step in the Frame tarball.
- WiVRn PR #1206: ccache; add it to oneVR's CI Android and Frame jobs.
- WiVRn PR #1087: a Vulkan AV1 encoder. Applies now, but upstream reports no image on
  NVIDIA and a crash on RDNA3.5. Take once merged.
- wayvr PR #703: holds the PipeWire buffer until drawn and waits on its fences, the cause
  behind `copy_captured_frames`. Conflicts with oneVR's capture changes (L). Once merged,
  test whether the copy is still needed, on the Xid 69 NVIDIA machine and on AMD.
- Not taken: wayvr #553 (oneVR places panels in front of the head) and #555 (oneVR forgets
  placement at each launch). Later, if merged: WiVRn #730, #1094, #1178; wayvr #476,
  #630, #695.

## Done and verified in CHANGES (reference)

Roadmap items now implemented (see CHANGES.md for the entries): WayVR #626 restart-in-place,
WayVR #628 Menu-button crash, WiVRn #1082 passthrough dark pixels, WiVRn PR #1033
supersampling, WiVRn #1153 swapchain-free fault, WiVRn #1119 Blender on hybrid GPU, WiVRn
#492 app pacer, "Starting game" feedback, WiVRn #491 see-through slider, WiVRn #967 stream-
ended reason, graphics presets (Potato to Rampage, Headset page), FOV sliders, per-game
controller type, AMD encoder quality, Library search/sort/recent, add any app, WiVRn #670
game fps in stats, headset charging and latency stages in the desktop window, USB
auto-connect, wake-on-LAN, hands+controllers together, virtual monitors. Headset-side CAS
sharpening covers the old "server-side sharpening" item. The Library browser entry was
shipped then removed this session.
