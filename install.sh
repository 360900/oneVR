#!/bin/sh
# Builds oneVR and installs it for this user, or updates an existing install.
set -eu

cd "$(dirname "$0")"
prefix="$HOME/.local/opt/onevr"
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

command -v cargo >/dev/null || { echo "install.sh: needs cargo (rustup)" >&2; exit 1; }

cargo xtask build --release --i686 --prefix "$prefix"

# The server can't be replaced while it runs.
if [ -x "$prefix/bin/onevr" ] && "$prefix/bin/onevr" status >/dev/null 2>&1; then
    echo "install.sh: stopping oneVR to update it"
    "$prefix/bin/onevr" stop
fi

cargo xtask install

mkdir -p "$bin" "$apps"
ln -sfn "$prefix/bin/onevr" "$bin/onevr"
for entry in io.github.onevr.desktop io.github.onevr.wayvr.desktop; do
    ln -sfn "$prefix/share/applications/$entry" "$apps/$entry"
done
command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps" || true

echo "install.sh: oneVR installed to $prefix"
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "install.sh: add $bin to PATH to run onevr from a terminal" ;;
esac

# The headset app: the latest release APK from CI, on each Quest adb reaches.
command -v adb >/dev/null && adb start-server >/dev/null 2>&1 || exit 0
quests=""
for serial in $(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'); do
    case "$(adb -s "$serial" shell getprop ro.product.manufacturer 2>/dev/null | tr -d '\r')" in
        Oculus | Meta*) quests="$quests $serial" ;;
    esac
done
[ -n "$quests" ] || exit 0

if ! command -v gh >/dev/null || ! gh auth status >/dev/null 2>&1; then
    echo "install.sh: a Quest is connected, but downloading the APK from CI needs gh, logged in with gh auth login" >&2
    exit 1
fi
repo=360900/oneVR
run=$(gh api "repos/$repo/actions/artifacts?name=onevr-apk-release&per_page=30" \
    -q '[.artifacts[] | select(.expired | not) | select(.workflow_run.head_branch == "main")][0].workflow_run.id // empty')
if [ -z "$run" ]; then
    echo "install.sh: CI has no onevr-apk-release build of main to install" >&2
    exit 1
fi
apkdir=$(mktemp -d)
trap 'rm -rf "$apkdir"' EXIT
echo "install.sh: downloading the headset APK from CI run $run"
gh run download "$run" --repo "$repo" --name onevr-apk-release --dir "$apkdir"
apk=$(find "$apkdir" -name '*.apk' | head -n 1)
[ -n "$apk" ] || { echo "install.sh: CI run $run has no APK" >&2; exit 1; }
for serial in $quests; do
    echo "install.sh: installing the headset app on $serial"
    "$prefix/bin/onevr" install "$apk" "$serial"
done
