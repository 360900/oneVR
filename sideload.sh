#!/bin/sh
# Installs the latest headset APK from CI on each Quest adb reaches.
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
    # A headset that hasn't allowed this PC yet shows up as unauthorized
    if adb devices | awk 'NR > 1 && $2 == "unauthorized" { found = 1 } END { exit !found }'; then
        say "a device waits for USB debugging to be allowed, accept it on the headset and run sideload.sh again"
    else
        say "no Quest connected over adb, headset app not installed"
    fi
    exit 0
fi

if ! command -v gh >/dev/null || ! gh auth status >/dev/null 2>&1; then
    fail "downloading the APK from CI needs gh, logged in with gh auth login"
fi
repo=360900/oneVR
run=$(gh api "repos/$repo/actions/artifacts?name=onevr-apk-release&per_page=30" \
    -q '[.artifacts[] | select(.expired | not) | select(.workflow_run.head_branch == "main")][0].workflow_run.id // empty')
[ -n "$run" ] || fail "CI has no onevr-apk-release build of main to install"
apkdir=$(mktemp -d)
trap 'rm -rf "$apkdir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
say "downloading the headset APK from CI run $run"
gh run download "$run" --repo "$repo" --name onevr-apk-release --dir "$apkdir" ||
    fail "the APK of CI run $run didn't download, see https://www.githubstatus.com if GitHub is down"
apk=$(find "$apkdir" -name '*.apk' | head -n 1)
[ -n "$apk" ] || fail "CI run $run has no APK"

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
