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

command -v cargo >/dev/null || fail "needs cargo (rustup)"

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
