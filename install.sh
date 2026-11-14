#!/bin/sh
# Builds oneVR and installs it for this user, or updates an existing install.
# The PC side is all or nothing: if a step fails, the earlier ones are undone.
set -eu

cd "$(dirname "$0")"
prefix="$HOME/.local/opt/onevr"
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
# Next to the prefix, so the swap is a rename on one filesystem
staging="$prefix.staging"
old="$prefix.old"
links="$bin/onevr=$prefix/bin/onevr
$apps/io.github.onevr.desktop=$prefix/share/applications/io.github.onevr.desktop
$apps/io.github.onevr.wayvr.desktop=$prefix/share/applications/io.github.onevr.wayvr.desktop"

say() { echo "install.sh: $*"; }
fail() { echo "install.sh: $*" >&2; exit 1; }

running() { [ -x "$prefix/bin/onevr" ] && "$prefix/bin/onevr" status >/dev/null 2>&1; }

# oneVR's window or tray icon, which a stop closes
ui_open() {
    command -v busctl >/dev/null &&
        busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
            org.freedesktop.DBus NameHasOwner s io.github.onevr.Ui 2>/dev/null |
        grep -q true
}

# Starts oneVR again as it was: with the tray if it had the window or tray.
# The server first, so its errors show here.
start_again() {
    "$prefix/bin/onevr" start || return
    if [ -n "$had_ui" ]; then
        setsid -f "$prefix/bin/onevr" --tray </dev/null >/dev/null 2>&1
    fi
}

refresh_menu() {
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$apps" || true
    fi
}

# What to undo if the script stops before the PC side is done
was_running=""
had_ui=""
moved_old=""
moved_new=""
saved_links=""
linked=""
done_pc=""

rollback() {
    say "install failed, undoing it"
    if [ -n "$linked" ]; then
        printf '%s\n' "$links" | while IFS='=' read -r link target; do
            saved=$(printf '%s\n' "$saved_links" | sed -n "s|^$link=||p")
            if [ -n "$saved" ]; then
                ln -sfn "$saved" "$link" || true
            elif [ -L "$link" ] && [ "$(readlink "$link")" = "$target" ]; then
                rm -f "$link"
            fi
        done
        refresh_menu
    fi
    if [ -n "$moved_new" ]; then rm -rf "$prefix"; fi
    if [ -n "$moved_old" ]; then mv "$old" "$prefix"; fi
    if [ -n "$was_running" ] && ! running; then
        say "starting oneVR again"
        start_again || true
    fi
    if [ -e "$prefix/bin/onevr" ]; then
        say "the previous oneVR is in place"
    else
        say "nothing was installed"
    fi
}

cleanup() {
    status=$?
    trap - EXIT INT TERM
    # Before the stop, nothing has changed
    if [ "$status" -ne 0 ] && [ -z "$done_pc" ] && [ -n "$was_running$moved_old$moved_new$linked" ]; then
        rollback
    fi
    rm -rf "$staging"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Build dependencies. Arch takes the PKGBUILD's; Fedora's are files and
# pkgconfig() names, so they don't depend on package names.
deps_fedora="/usr/bin/cmake /usr/bin/ninja /usr/bin/git /usr/bin/g++ /usr/bin/pkg-config
/usr/bin/python3 /usr/bin/glslc /usr/bin/glslangValidator /usr/bin/spirv-opt
/usr/bin/gdbus-codegen clang-devel boost-devel eigen3-devel pkgconfig(nlohmann_json)
pkgconfig(CLI11) pkgconfig(glib-2.0) pkgconfig(alsa) pkgconfig(avahi-client) pkgconfig(avahi-glib)
pkgconfig(dav1d) pkgconfig(dbus-1) pkgconfig(gl) pkgconfig(libarchive) pkgconfig(libavcodec)
pkgconfig(libavfilter) pkgconfig(libavutil) pkgconfig(libbsd) pkgconfig(libcap)
pkgconfig(libcjson) pkgconfig(libdrm) pkgconfig(libinput) pkgconfig(libnotify)
pkgconfig(libpipewire-0.3) pkgconfig(libpng) pkgconfig(librsvg-2.0) pkgconfig(libswscale)
pkgconfig(libsystemd) pkgconfig(libudev) pkgconfig(libva) pkgconfig(openssl) pkgconfig(openxr)
pkgconfig(shaderc) pkgconfig(vulkan) pkgconfig(wayland-client) pkgconfig(wayland-protocols)
pkgconfig(wayland-scanner) pkgconfig(x11) pkgconfig(x11-xcb) pkgconfig(xcb) pkgconfig(xcb-randr)
pkgconfig(xcursor) pkgconfig(xext) pkgconfig(xi) pkgconfig(xkbcommon) pkgconfig(xkbcommon-x11)
pkgconfig(xrandr) glibc-devel(x86-32) libstdc++-devel(x86-32) libatomic(x86-32)
libX11-devel(x86-32)"
deps_debian="build-essential gcc-multilib g++-multilib cmake ninja-build git pkg-config python3
bsdextrautils clang libclang-dev glslang-tools glslc spirv-tools libshaderc-dev libboost-dev
libboost-locale-dev libboost-url-dev libboost-iostreams-dev libeigen3-dev nlohmann-json3-dev
libcli11-dev libglib2.0-dev libglib2.0-dev-bin libvulkan-dev libasound2-dev libavahi-client-dev
libavahi-glib-dev libdav1d-dev libdbus-1-dev libgl-dev libglx-dev libegl-dev libopengl-dev
mesa-common-dev libarchive-dev libavcodec-dev libavfilter-dev libavutil-dev libbsd-dev libcap-dev
libcjson-dev libdrm-dev libinput-dev libnotify-dev libpipewire-0.3-dev libpng-dev librsvg2-dev
libswscale-dev libsystemd-dev libudev-dev libva-dev libssl-dev libopenxr-dev libx264-dev
libwayland-dev wayland-protocols libx11-dev libx11-xcb-dev libxcb1-dev libxcb-randr0-dev
libxcursor-dev libxext-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-dev libxrandr-dev
libx11-dev:i386"

# Root for the package manager
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif command -v sudo >/dev/null; then
        sudo "$@"
    elif command -v doas >/dev/null; then
        doas "$@"
    else
        fail "needs sudo or doas to install: $missing"
    fi
}

# Installs the missing build dependencies with the distribution's package
# manager. Prompts are left to it, unless there is no terminal to answer them.
install_deps() {
    distro=""
    if [ -r /etc/os-release ]; then
        # shellcheck disable=SC1091
        distro=$(. /etc/os-release && echo "${ID:-} ${ID_LIKE:-}")
    fi
    family=""
    for id in $distro; do
        case "$id" in
            arch) family=arch ;;
            debian | ubuntu) family=debian ;;
            fedora) family=fedora ;;
            *) continue ;;
        esac
        break
    done
    yes=""
    [ -t 0 ] || yes=1
    missing=""
    # Also from rustup.rs, in ~/.cargo/bin
    rustup=rustup
    if command -v rustup >/dev/null; then rustup=""; fi
    case "$family" in
        arch)
            command -v pacman >/dev/null || family=""
            ;;
        debian)
            command -v apt-get >/dev/null || family=""
            ;;
        fedora)
            command -v dnf >/dev/null || family=""
            ;;
    esac
    case "$family" in
        arch)
            # makepkg expects base-devel; the lib32 packages are in multilib
            missing=$(bash -c '. packaging/arch/PKGBUILD && pacman -T base-devel "${makedepends[@]}" "${depends[@]}"' |
                grep -vx rustup | tr '\n' ' ') || true
            missing="$missing$rustup"
            if [ -n "$missing" ]; then
                case "$missing" in
                    *lib32-*) pacman -Si lib32-glibc >/dev/null 2>&1 ||
                        fail "needs the multilib repository for 32-bit games, enable it in /etc/pacman.conf" ;;
                esac
                say "installing build dependencies: $missing"
                # shellcheck disable=SC2086
                as_root pacman -S --needed ${yes:+--noconfirm} $missing
            fi
            ;;
        debian)
            for p in $deps_debian; do
                dpkg-query -W -f='${db:Status-Abbrev}' "$p" 2>/dev/null | grep -q '^ii' ||
                    missing="$missing $p"
            done
            missing="$missing${rustup:+ rustup}"
            if [ -n "$missing" ]; then
                say "installing build dependencies:$missing"
                # The 32-bit xrizer's OpenXR loader needs the i386 X11
                # package, Steam needs i386 too
                if ! dpkg --print-foreign-architectures | grep -qx i386; then
                    as_root dpkg --add-architecture i386
                fi
                as_root apt-get update
                # shellcheck disable=SC2086
                as_root apt-get install ${yes:+-y} $missing
            fi
            ;;
        fedora)
            for p in $deps_fedora; do
                rpm -q --whatprovides "$p" >/dev/null 2>&1 || missing="$missing $p"
            done
            missing="$missing${rustup:+ rustup}"
            # x264 is in RPM Fusion, if enabled
            if ! rpm -q --whatprovides 'pkgconfig(x264)' >/dev/null 2>&1 &&
                dnf -q repoquery --whatprovides 'pkgconfig(x264)' 2>/dev/null | grep -q .; then
                missing="$missing pkgconfig(x264)"
            fi
            if [ -n "$missing" ]; then
                say "installing build dependencies:$missing"
                # shellcheck disable=SC2086
                as_root dnf install ${yes:+-y} $missing
            fi
            ;;
        *)
            say "unknown distribution, install the build dependencies of WiVRn, xrizer and wayvr yourself"
            ;;
    esac
}
PATH="$HOME/.cargo/bin:$PATH"
install_deps

# The shaderc crate looks for libshaderc_shared.so, which Debian names
# libshaderc.so, and otherwise links Debian's libshaderc_combined.a, which
# lacks glslang.
debian_shaderc=/usr/lib/x86_64-linux-gnu/libshaderc.so
if [ -e "$debian_shaderc" ] && [ ! -e "${debian_shaderc%.so}_shared.so" ]; then
    mkdir -p target/shaderc
    ln -sfn "$debian_shaderc" target/shaderc/libshaderc_shared.so
    SHADERC_LIB_DIR="$PWD/target/shaderc"
    export SHADERC_LIB_DIR
fi

# Distribution rustup packages may only ship rustup-init, which installs
# rustup into ~/.cargo.
if ! command -v rustup >/dev/null && command -v rustup-init >/dev/null; then
    rustup-init -y --no-modify-path --default-toolchain none
fi
command -v cargo >/dev/null || fail "needs cargo (rustup)"
# Installs the toolchain and targets in rust-toolchain.toml. Before rustup
# 1.28 that takes running cargo.
if command -v rustup >/dev/null; then
    rustup toolchain install 2>/dev/null || cargo --version >/dev/null
fi

# A run killed during the swap leaves the previous install aside
if [ -e "$old" ]; then
    if [ -e "$prefix" ]; then rm -rf "$old"; else mv "$old" "$prefix"; fi
fi
rm -rf "$staging"

# Links can be restored, files in their place can't
printf '%s\n' "$links" | while IFS='=' read -r link target; do
    if [ -e "$link" ] && [ ! -L "$link" ]; then
        fail "$link is in the way, it isn't a link"
    fi
done

if [ -e "$prefix/bin/onevr" ]; then action=updated; else action=installed; fi

# The server can't be replaced while it runs. Asked before the build, so the
# answer doesn't wait on it, and again if oneVR started during the build.
stop_ok=""
ask_stop() {
    [ -n "$stop_ok" ] && return
    [ -t 0 ] || fail "oneVR is running, stop it to update"
    printf "install.sh: oneVR is running and has to stop to update. Stop it? [y/N] "
    read -r answer || answer=""
    case "$answer" in
        [yY] | [yY][eE][sS]) stop_ok=1 ;;
        *) fail "oneVR not updated" ;;
    esac
}
if running; then ask_stop; fi

cargo xtask build --release --i686 --prefix "$prefix"
cargo xtask install --destdir "$staging"

if running; then
    ask_stop
    was_running=1
    if ui_open; then had_ui=1; fi
    "$prefix/bin/onevr" stop
fi

mkdir -p "$(dirname "$prefix")" "$bin" "$apps"
if [ -e "$prefix" ]; then
    mv "$prefix" "$old"
    moved_old=1
fi
mv "$staging$prefix" "$prefix"
moved_new=1

saved_links=$(printf '%s\n' "$links" | while IFS='=' read -r link target; do
    if [ -L "$link" ]; then echo "$link=$(readlink "$link")"; fi
done)
linked=1
printf '%s\n' "$links" | while IFS='=' read -r link target; do
    ln -sfn "$target" "$link"
done
refresh_menu

done_pc=1
rm -rf "$old"
say "oneVR $action in $prefix"
if [ -n "$was_running" ]; then
    say "starting oneVR again"
    start_again || echo "install.sh: oneVR didn't start again, see onevr logs" >&2
fi
case ":$PATH:" in
    *":$bin:"*) ;;
    *) say "add $bin to PATH to run onevr from a terminal" ;;
esac

# The headset app, if a Quest is connected
if command -v adb >/dev/null; then
    ./sideload.sh
else
    say "adb isn't installed, headset app not installed"
fi
