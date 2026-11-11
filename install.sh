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

refresh_menu() {
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$apps" || true
    fi
}

# What to undo if the script stops before the PC side is done
was_running=""
moved_old=""
moved_new=""
saved_links=""
linked=""
done_pc=""
apkdir=""

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
        "$prefix/bin/onevr" start || true
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
    if [ -n "$apkdir" ]; then rm -rf "$apkdir"; fi
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
case ":$PATH:" in
    *":$bin:"*) ;;
    *) say "add $bin to PATH to run onevr from a terminal" ;;
esac

# The headset app: the latest release APK from CI, on each Quest adb reaches.
# A failed adb install leaves the headset's app as it was.
if ! command -v adb >/dev/null || ! adb start-server >/dev/null 2>&1; then
    say "adb isn't available, headset app not installed"
    exit 0
fi
quests=""
for serial in $(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'); do
    case "$(adb -s "$serial" shell getprop ro.product.manufacturer 2>/dev/null | tr -d '\r')" in
        Oculus | Meta*) quests="$quests $serial" ;;
    esac
done
if [ -z "$quests" ]; then
    say "no Quest connected over adb, headset app not installed"
    exit 0
fi

if ! command -v gh >/dev/null || ! gh auth status >/dev/null 2>&1; then
    fail "a Quest is connected, but downloading the APK from CI needs gh, logged in with gh auth login"
fi
repo=360900/oneVR
run=$(gh api "repos/$repo/actions/artifacts?name=onevr-apk-release&per_page=30" \
    -q '[.artifacts[] | select(.expired | not) | select(.workflow_run.head_branch == "main")][0].workflow_run.id // empty')
[ -n "$run" ] || fail "CI has no onevr-apk-release build of main to install"
apkdir=$(mktemp -d)
say "downloading the headset APK from CI run $run"
gh run download "$run" --repo "$repo" --name onevr-apk-release --dir "$apkdir"
apk=$(find "$apkdir" -name '*.apk' | head -n 1)
[ -n "$apk" ] || fail "CI run $run has no APK"
for serial in $quests; do
    model=$(adb -s "$serial" shell getprop ro.product.model 2>/dev/null | tr -d '\r')
    say "installing $(basename "$apk") on ${model:-the headset} ($serial)"
    "$prefix/bin/onevr" install "$apk" "$serial" || fail "headset app not installed on ${model:-the headset} ($serial)"
    say "headset app installed on ${model:-the headset} ($serial)"
done
