#!/bin/sh
# Installs the latest headset APK, CI's build of main, on each Quest adb reaches.
set -eu

say() { echo "sideload.sh: $*"; }
fail() { echo "sideload.sh: $*" >&2; exit 1; }

command -v adb >/dev/null || fail "needs adb (android-tools)"
adb start-server >/dev/null 2>&1 || fail "adb didn't start"

quests=""
for serial in $(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'); do
    case "$(adb -s "$serial" shell getprop ro.product.manufacturer 2>/dev/null | tr -d '\r')" in
        Oculus | Meta*) quests="$quests $serial" ;;
    esac
done
if [ -z "$quests" ]; then
    # adb's states for a device it can't use yet
    case " $(adb devices | awk 'NR > 1 && NF { printf "%s ", $2 }')" in
        *" unauthorized "*)
            say "accept USB debugging on the headset, then run sideload.sh again" ;;
        *" no "*)
            say "adb has no permission to open the headset, install your distribution's udev rules for adb (such as android-udev) and plug it in again" ;;
        *" offline "*)
            say "the headset's adb connection is offline, plug it in again" ;;
        *)
            say "adb finds no Quest: plug it in by USB with a data cable, with developer mode on (the oneVR connection over Wi-Fi isn't adb)" ;;
    esac
    say "headset app not installed"
    exit 0
fi

command -v curl >/dev/null || fail "needs curl"
# The release CI updates with each build of main
url=https://github.com/360900/oneVR/releases/download/latest/onevr.apk
apkdir=$(mktemp -d)
trap 'rm -rf "$apkdir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
apk="$apkdir/onevr.apk"
say "downloading the latest headset APK"
curl -fL --retry 3 --progress-bar -o "$apk" "$url" ||
    fail "the APK didn't download from $url"

# A failed adb install leaves the headset's app as it was.
failed=""
for serial in $quests; do
    model=$(adb -s "$serial" shell getprop ro.product.model 2>/dev/null | tr -d '\r')
    headset="${model:-the headset} ($serial)"
    say "installing $(basename "$apk") on $headset"
    if adb -s "$serial" install -r "$apk"; then
        say "headset app installed on $headset"
    else
        echo "sideload.sh: headset app not installed on $headset" >&2
        failed=1
    fi
done
[ -z "$failed" ]
