# oneVR

Stream VR from a Linux PC to a standalone headset, run SteamVR games without SteamVR and use your desktop in VR, with one app and one headset APK.

oneVR combines:

- [WiVRn](https://github.com/WiVRn/WiVRn) (`wivrn/`): streaming server and headset client
- [xrizer-edge](https://github.com/360900/xrizer-edge) (`xrizer/`): OpenVR on top of OpenXR
- [wayvr](https://github.com/wayvr-org/wayvr) (`wayvr/`): desktop overlay

# Install

Arch Linux:

```sh
cd packaging/arch && makepkg -si
```

This installs to `/opt/onevr` and adds `onevr` to the app menu. It doesn't conflict with system WiVRn or wayvr packages.

Other distributions: install the build dependencies of WiVRn, xrizer and wayvr, then:

```sh
cargo xtask build --release --i686 --prefix "$HOME/.local/opt/onevr"
cargo xtask install
mkdir -p ~/.local/bin ~/.local/share/applications
ln -sf "$HOME/.local/opt/onevr/bin/onevr" ~/.local/bin/
ln -sf "$HOME/.local/opt/onevr/share/applications/io.github.onevr.desktop" ~/.local/share/applications/
ln -sf "$HOME/.local/opt/onevr/share/applications/io.github.onevr.wayvr.desktop" ~/.local/share/applications/
```

The prefix is fixed at build time. The links put `onevr` in `~/.local/bin` and oneVR in the app menu. On Plasma before 6.8, the `io.github.onevr.wayvr.desktop` link lets oneVR capture screens without a dialog and add virtual monitors.

The headset APK is the `onevr-apk-release` artifact of [CI](https://github.com/360900/oneVR/actions/workflows/ci.yml).

# Usage

Run `onevr` (or oneVR from the app menu) to start the server in the background and open oneVR's window, with pairing and the headset and server settings; closing it leaves the server and a tray icon. Open oneVR on the headset and pick this PC; the first time, enter the PIN the PC shows. Once connected, the settings are in the dashboard in VR too. OpenVR games use the bundled xrizer.

oneVR keeps its server settings in `~/.config/onevr/wivrn-server.json`. Pairing is shared with WiVRn.

- `onevr start`: the server without the window; `onevr stop`, `onevr status`, `onevr logs`
- `onevr pair`: pair another headset
- `onevr headsets`, `onevr rename`, `onevr unpair`: manage paired headsets
- `onevr usb`: connect the headset plugged in by USB; `onevr install APK` installs the app
- `onevr steam`: launch options for Steam VR games, if they need any
- `onevr doctor`: check the PC for VR (network, GPU, runtimes, Steam, input, audio); `--fix` repairs what it can, the first start offers to open the firewall and start Avahi
- `onevr server`: the server in the foreground

# Development

```sh
cargo xtask run
```

This builds into `target/stage`, installs there and starts its `onevr`. Build options such as `--i686` go first, arguments after `--` go to `onevr`.

`cargo xtask apk --debug` builds an installable headset APK and needs the Android SDK and NDK. Upstream changes are merged as described in [CHANGES.md](CHANGES.md), followed by `cargo xtask rebrand`.

# License

GPL-3.0. Each component keeps its own license files.
