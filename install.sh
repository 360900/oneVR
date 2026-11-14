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

say() { echo "$*"; }
fail() { echo "Error: $*" >&2; exit 1; }
# A path with ~ for the home folder, for messages
tilde() {
    case "$1" in
        "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;;
        *) printf '%s' "$1" ;;
    esac
}

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
building=""

rollback() {
    say "The install failed, undoing it."
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
        say "Starting oneVR again."
        start_again || true
    fi
    if [ -e "$prefix/bin/onevr" ]; then
        say "The previous oneVR is still in place."
    else
        say "Nothing was installed."
    fi
}

cleanup() {
    status=$?
    trap - EXIT INT TERM
    # An interrupted build leaves the progress bar's line open
    if [ -n "$building" ] && [ -t 1 ]; then echo; fi
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
deps_debian="build-essential cmake ninja-build git pkg-config python3
bsdextrautils clang libclang-dev glslang-tools glslc spirv-tools libshaderc-dev libboost-dev
libboost-locale-dev libboost-url-dev libboost-iostreams-dev libeigen3-dev nlohmann-json3-dev
libcli11-dev libglib2.0-dev libglib2.0-dev-bin libvulkan-dev libasound2-dev libavahi-client-dev
libavahi-glib-dev libdav1d-dev libdbus-1-dev libgl-dev libglx-dev libegl-dev libopengl-dev
mesa-common-dev libarchive-dev libavcodec-dev libavfilter-dev libavutil-dev libbsd-dev libcap-dev
libcjson-dev libdrm-dev libinput-dev libnotify-dev libpipewire-0.3-dev libpng-dev librsvg2-dev
libswscale-dev libsystemd-dev libudev-dev libva-dev libssl-dev libopenxr-dev libx264-dev
libwayland-dev wayland-protocols libx11-dev libx11-xcb-dev libxcb1-dev libxcb-randr0-dev
libxcursor-dev libxext-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-dev libxrandr-dev"
# For the 32-bit xrizer; its OpenXR loader needs the i386 X11 package
deps_debian_i686="gcc-multilib g++-multilib libx11-dev:i386"

# Builds the 32-bit xrizer too, for 32-bit games
i686=1

# Asks a yes or no question, yes by default. Without a terminal to answer
# it, the answer is no.
confirm() {
    [ -t 0 ] || return 1
    printf "%s [Y/n] " "$1"
    read -r answer || answer=""
    case "$answer" in
        "" | [yY] | [yY][eE][sS]) return 0 ;;
        *) return 1 ;;
    esac
}

# Root for the package manager
as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif command -v sudo >/dev/null; then
        sudo "$@"
    elif command -v doas >/dev/null; then
        doas "$@"
    else
        fail "installing $missing needs sudo or doas."
    fi
}

has_x264_repo() {
    dnf -q repoquery --whatprovides 'pkgconfig(x264)' 2>/dev/null | grep -q .
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
                        fail "32-bit games need the multilib repository. Enable it in /etc/pacman.conf." ;;
                esac
                say "Installing build dependencies: $missing"
                # shellcheck disable=SC2086
                as_root pacman -S --needed ${yes:+--noconfirm} $missing
            fi
            ;;
        debian)
            if ! dpkg --print-foreign-architectures | grep -qx i386; then
                say "32-bit games, such as Half-Life 2: VR Mod, need 32-bit packages from apt's i386"
                say "architecture, which Steam also uses. Without it, only 64-bit games work in VR."
                if confirm "Enable i386?"; then
                    as_root dpkg --add-architecture i386
                else
                    say "Building for 64-bit games only."
                    i686=""
                fi
            fi
            if [ -n "$i686" ]; then deps_debian="$deps_debian $deps_debian_i686"; fi
            for p in $deps_debian; do
                dpkg-query -W -f='${db:Status-Abbrev}' "$p" 2>/dev/null | grep -q '^ii' ||
                    missing="$missing $p"
            done
            missing="$missing${rustup:+ rustup}"
            if [ -n "$missing" ]; then
                say "Installing build dependencies:$missing"
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
            # x264 is in RPM Fusion
            if ! rpm -q --whatprovides 'pkgconfig(x264)' >/dev/null 2>&1; then
                if ! has_x264_repo; then
                    rpmfusion=https://mirrors.rpmfusion.org/free/fedora
                    rpmfusion="$rpmfusion/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm"
                    say "CPU encoding (x264) needs the RPM Fusion repository. It's highly"
                    say "recommended: it streams when the GPU can't encode the video. Enabling it runs:"
                    say "  sudo dnf install $rpmfusion"
                    if confirm "Enable RPM Fusion?"; then
                        as_root dnf install ${yes:+-y} "$rpmfusion"
                    fi
                fi
                if has_x264_repo; then
                    missing="$missing pkgconfig(x264)"
                else
                    say "Building without CPU encoding (x264)."
                fi
            fi
            if [ -n "$missing" ]; then
                say "Installing build dependencies:$missing"
                # shellcheck disable=SC2086
                as_root dnf install ${yes:+-y} $missing
            fi
            ;;
        *)
            say "Unknown distribution: install the build dependencies of WiVRn, xrizer and wayvr yourself."
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

# A run killed during the swap leaves the previous install aside
if [ -e "$old" ]; then
    if [ -e "$prefix" ]; then rm -rf "$old"; else mv "$old" "$prefix"; fi
fi
rm -rf "$staging"

# Links can be restored, files in their place can't
printf '%s\n' "$links" | while IFS='=' read -r link target; do
    if [ -e "$link" ] && [ ! -L "$link" ]; then
        fail "$link is in the way and isn't a link."
    fi
done

if [ -e "$prefix/bin/onevr" ]; then action=updated; else action=installed; fi

# The server can't be replaced while it runs. Asked before the build, so the
# answer doesn't wait on it, and again if oneVR started during the build.
stop_ok=""
ask_stop() {
    [ -n "$stop_ok" ] && return
    [ -t 0 ] || fail "oneVR is running. Stop it to update."
    printf "oneVR is running and has to stop to update. Stop it? [y/N] "
    read -r answer || answer=""
    case "$answer" in
        [yY] | [yY][eE][sS]) stop_ok=1 ;;
        *) fail "oneVR wasn't updated." ;;
    esac
}
if running; then ask_stop; fi

# Run with their output in the log, each step checked: set -e doesn't apply
# to a function called with ||.
build() {
    echo "install.sh: step rust"
    # Distribution rustup packages may only ship rustup-init, which installs
    # rustup into ~/.cargo.
    if ! command -v rustup >/dev/null && command -v rustup-init >/dev/null; then
        rustup-init -y --no-modify-path --default-toolchain none || return
    fi
    if ! command -v cargo >/dev/null; then
        echo "Error: building oneVR needs cargo (rustup)."
        return 1
    fi
    # Installs the toolchain and targets in rust-toolchain.toml. Before rustup
    # 1.28 that takes running cargo.
    if command -v rustup >/dev/null; then
        rustup toolchain install || cargo --version || return
    fi
    # Cargo's progress bar, which it otherwise only shows on a terminal
    CARGO_TERM_PROGRESS_WHEN=always
    CARGO_TERM_PROGRESS_WIDTH=100
    CARGO_TERM_COLOR=never
    export CARGO_TERM_PROGRESS_WHEN CARGO_TERM_PROGRESS_WIDTH CARGO_TERM_COLOR
    echo "install.sh: step build"
    cargo xtask build --release ${i686:+--i686} --prefix "$prefix" || return
    echo "install.sh: step install"
    cargo xtask install --destdir "$staging"
}

# Turns the build's output into one progress bar on a terminal, or a line per
# step otherwise, and writes it to the log. Its last line is the build's exit
# status, which this exits with. The weights are each step's share of a full
# build's time.
progress_awk='
BEGIN {
    n = split("rust prep xrizer xrizer32 wivrn-config wivrn wayvr onevr install", keys, " ")
    split("2 2 9 8 2 33 34 6 4", weights, " ")
    split("setting up Rust|preparing the build|building xrizer|building the 32-bit xrizer|configuring WiVRn|building WiVRn|building wayvr|building oneVR|installing", labels, "|")
    total = 0
    for (i = 1; i <= n; i++) {
        k = keys[i]
        weight[k] = (k == "xrizer32" && !i686) ? 0 : weights[i]
        start[k] = total
        label[k] = labels[i]
        total += weight[k]
    }
    cargo_steps["xrizer"] = cargo_steps["xrizer32"] = cargo_steps["wayvr"] = cargo_steps["onevr"] = 1
    stage = "rust"; frac = 0; pct = 0; detail = ""; status = 1; warnings = 0; notes = ""
    if (utf8) { full = "\342\226\210"; empty = "\342\226\221" } else { full = "#"; empty = "-" }
    barw = 24
}

function set_stage(s) {
    if (s != stage && s in weight) { stage = s; frac = 0; detail = "" }
}

function draw(   p, text, room, filled, bar, i, line) {
    if (!tty) {
        if (stage != shown) { print toupper(substr(label[stage], 1, 1)) substr(label[stage], 2) "..."; shown = stage }
        return
    }
    p = int(100 * (start[stage] + weight[stage] * frac) / total)
    if (p > 100) p = 100
    if (p > pct) pct = p
    text = toupper(substr(label[stage], 1, 1)) substr(label[stage], 2)
    if (detail != "") text = text ": " detail
    room = cols - barw - 8
    if (room < 0) room = 0
    if (length(text) > room) text = substr(text, 1, room)
    filled = int(barw * pct / 100)
    bar = ""
    for (i = 0; i < barw; i++) bar = bar (i < filled ? full : empty)
    line = sprintf("%s %3d%%  %s", bar, pct, text)
    if (line != last) { printf "\r%s\033[K", line; fflush(); last = line }
}

/^install\.sh: status [0-9]+$/ { status = $3 + 0; next }

# Cargo redraws its progress bar, which would fill the log
!/^ +Building \[/ { print > logfile }

/^warning: / { if (!/generated [0-9]+ warnings?/ && !/^warning: build failed/) warnings++ }
# CMake warnings are for the developers of the projects
/: warning: / { warnings++ }

/^install\.sh: step / { s = $3; if (s == "build") s = "prep"; set_stage(s); draw(); next }
/^xtask: building / { s = $3; if (s == "wivrn") s = "wivrn-config"; set_stage(s); draw(); next }
/^xtask: / {
    if (!/^xtask: (installed to|removed) /) {
        d = substr($0, 8)
        notes = notes toupper(substr(d, 1, 1)) substr(d, 2) "\n"
    }
    next
}

# The commands xtask runs: the second xrizer is the 32-bit one
/^\+ / {
    if (stage == "xrizer" && index($0, "\"--target\"")) set_stage("xrizer32")
    else if (stage == "wivrn-config" && index($0, "\"--build\"")) set_stage("wivrn")
    draw(); next
}

/^ +Building \[/ && stage in cargo_steps {
    if (match($0, /[0-9]+\/[0-9]+/)) {
        split(substr($0, RSTART, RLENGTH), nm, "/")
        if (nm[2] > 0) frac = nm[1] / nm[2]
        d = substr($0, RSTART + RLENGTH)
        sub(/^: */, "", d)
        sub(/[,( ].*/, "", d)
        sub("\342\200\246.*", "", d)
        if (d != "") detail = d
    }
    draw(); next
}
/^ +Compiling / && stage in cargo_steps { detail = $2; draw(); next }

# Ninja: [done/total] description
/^\[[0-9]+\/[0-9]+\] / && stage == "wivrn" {
    split(substr($1, 2, length($1) - 2), nm, "/")
    if (nm[2] > 0) frac = nm[1] / nm[2]
    d = $NF
    sub(/.*\//, "", d)
    sub(/\.o$/, "", d)
    if ($2 == "Linking") d = "linking " d
    detail = d
    draw(); next
}

/^info: (downloading|installing) component / {
    d = $0
    sub(/^info: /, "", d)
    gsub(/\047/, "", d)
    detail = d
    draw(); next
}
/^-- Installing: / { d = $NF; sub(/.*\//, "", d); detail = d; draw(); next }

END {
    close(logfile)
    if (tty) {
        if (status == 0) { stage = "install"; frac = 1; label[stage] = "done"; detail = ""; draw() }
        printf "\n"
    }
    if (status == 0) {
        if (warnings == 1) print "Built with 1 warning, see " logname "."
        else if (warnings > 1) print "Built with " warnings " warnings, see " logname "."
        printf "%s", notes
    }
    exit status
}
'

progress() {
    tty=""
    cols=80
    if [ -t 1 ]; then
        tty=1
        size=$({ stty size </dev/tty; } 2>/dev/null) || size=""
        case "${size#* }" in
            "" | *[!0-9]*) ;;
            *) if [ "${size#* }" -ge 40 ]; then cols=${size#* }; fi ;;
        esac
    fi
    utf8=""
    if [ "$(locale charmap 2>/dev/null)" = UTF-8 ]; then utf8=1; fi
    # Cargo ends its progress lines with a carriage return. tr and mawk hold
    # their output and input in a buffer, which would hold the bar back.
    unbuffered=""
    if command -v stdbuf >/dev/null; then unbuffered="stdbuf -o0"; fi
    awk_flags=""
    case "$(awk -W version 2>&1 | sed -n 1p)" in
        mawk*) awk_flags="-W interactive" ;;
    esac
    # shellcheck disable=SC2086
    $unbuffered tr '\r' '\n' | awk $awk_flags -v tty="$tty" -v cols="$cols" -v utf8="$utf8" \
        -v i686="$i686" -v logfile="$build_log" -v logname="$(tilde "$build_log")" "$progress_awk"
}

# The errors in a failed build's log: compiler and linker errors, CMake's and
# Ninja's failed steps, and what xtask and this script said.
show_errors() {
    found=$(awk '
        # Ninja follows FAILED with the command line, which is long and of no help
        skip { skip = 0; next }
        /^FAILED: / { print; block = 1; skip = 1; next }
        /^(error(\[[A-Z0-9]+\])?|CMake Error)[: ]/ { block = 1 }
        /^(xtask: |Error: )/ && !/^xtask: building / { print; block = 0; next }
        /^\[[0-9]+\/[0-9]+\] / || /^ninja: build stopped/ || /^-- / { block = 0 }
        /^ +(Compiling|Finished|Running|Fresh) / || /^warning/ { block = 0 }
        block { print }
    ' "$build_log")
    if [ -z "$found" ]; then found=$(tail -n 30 "$build_log"); fi
    lines=$(printf '%s\n' "$found" | wc -l)
    printf '%s\n' "$found" | head -n 80 >&2
    if [ "$lines" -gt 80 ]; then echo "..." >&2; fi
}

mkdir -p target
build_log="$PWD/target/install.log"
# Without a terminal for the questions, the build doesn't wait on one either
building=1
if ! { rc=0; build </dev/null || rc=$?; echo "install.sh: status $rc"; } 2>&1 | progress; then
    building=""
    show_errors
    fail "the build failed. Its full log is in $(tilde "$build_log")."
fi
building=""

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
say "oneVR $action in $(tilde "$prefix")."
if [ -n "$was_running" ]; then
    say "Starting oneVR again."
    start_again || echo "Warning: oneVR didn't start again, see onevr logs." >&2
fi
case ":$PATH:" in
    *":$bin:"*) ;;
    *) say "To run onevr from a terminal, add $(tilde "$bin") to your PATH." ;;
esac

# The headset app
echo
if ! command -v adb >/dev/null; then
    say "To install the headset app on a Quest, install adb (often in a package"
    say "called android-tools), then run ./sideload.sh."
else
    say "To install the headset app on a Quest:"
    say "  1. Turn on developer mode in the Meta Horizon app on your phone."
    say "  2. Plug the Quest in by USB, put it on and allow USB debugging."
    if ! confirm "Install the headset app now?"; then
        say "Run ./sideload.sh to install it later."
    else
        ./sideload.sh
    fi
fi
