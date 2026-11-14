#!/bin/sh
# Installs the latest headset APK, CI's build of main, on each Quest adb reaches.
set -eu

say() { echo "$*"; }
fail() { echo "Error: $*" >&2; exit 1; }

command -v adb >/dev/null || fail "installing the headset app needs adb (android-tools)."

# A stale adb server can miss a headset plugged in after it started, so it
# goes whenever sideloading fails, and the next run starts a fresh one.
apkdir=""
cleanup() {
    status=$?
    trap - EXIT INT TERM
    if [ -n "$apkdir" ]; then rm -rf "$apkdir"; fi
    if [ "$status" -ne 0 ]; then adb kill-server >/dev/null 2>&1 || true; fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

find_quests() {
    adb start-server >/dev/null 2>&1 || fail "adb didn't start."
    quests=""
    for serial in $(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'); do
        case "$(adb -s "$serial" shell getprop ro.product.manufacturer 2>/dev/null | tr -d '\r')" in
            Oculus | Meta*) quests="$quests $serial" ;;
        esac
    done
}
find_quests
if [ -z "$quests" ]; then
    adb kill-server >/dev/null 2>&1 || true
    adb start-server >/dev/null 2>&1 || fail "adb didn't start."
    # A new server lists USB devices once it has scanned them
    timeout 3 adb wait-for-device >/dev/null 2>&1 || true
    find_quests
fi
if [ -z "$quests" ]; then
    # adb's states for a device it can't use yet
    case " $(adb devices | awk 'NR > 1 && NF { printf "%s ", $2 }')" in
        *" unauthorized "*)
            say "Allow USB debugging on the headset, then run ./sideload.sh again." ;;
        *" no "*)
            say "adb has no permission to open the headset. Install your distribution's udev rules for adb (such as android-udev) and plug it in again." ;;
        *" offline "*)
            say "The headset's adb connection is offline. Plug it in again." ;;
        *)
            say "No Quest found. Plug it in by USB with a data cable, with developer mode on (the oneVR connection over Wi-Fi isn't adb)." ;;
    esac
    say "The headset app wasn't installed."
    adb kill-server >/dev/null 2>&1 || true
    exit 0
fi

command -v curl >/dev/null || fail "installing the headset app needs curl."
# The latest of the releases CI makes of each build of main
url=https://github.com/360900/oneVR/releases/latest/download/onevr.apk
apkdir=$(mktemp -d)
apk="$apkdir/onevr.apk"
say "Downloading the latest headset app..."
curl -fL --retry 3 --progress-bar -o "$apk" "$url" ||
    fail "the headset app didn't download from $url."

# A failed adb install leaves the headset's app as it was.
failed=""
for serial in $quests; do
    model=$(adb -s "$serial" shell getprop ro.product.model 2>/dev/null | tr -d '\r')
    headset="${model:-the headset} ($serial)"
    say "Installing it on $headset..."
    # adb's output only matters when it fails
    if out=$(adb -s "$serial" install -r "$apk" 2>&1); then
        say "Headset app installed on $headset."
    else
        printf '%s\n' "$out" >&2
        echo "Error: the headset app wasn't installed on $headset." >&2
        failed=1
    fi
done
[ -z "$failed" ]
