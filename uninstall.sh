#!/bin/sh
# Removes the oneVR that install.sh installed. Settings and pairing are kept.
set -eu

prefix="$HOME/.local/opt/onevr"
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

# A path with ~ for the home folder, for messages
tilde() {
    case "$1" in
        "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;;
        *) printf '%s' "$1" ;;
    esac
}

if [ ! -e "$prefix" ]; then
    echo "Error: oneVR isn't installed in $(tilde "$prefix")." >&2
    exit 1
fi

if [ -x "$prefix/bin/onevr" ] && "$prefix/bin/onevr" status >/dev/null 2>&1; then
    "$prefix/bin/onevr" stop
fi

# Only the links install.sh made, not another install's.
unlink_ours() {
    if [ -L "$1" ] && [ "$(readlink "$1")" = "$2" ]; then
        rm -f "$1"
    fi
}
unlink_ours "$bin/onevr" "$prefix/bin/onevr"
for entry in io.github.onevr.desktop io.github.onevr.wayvr.desktop; do
    unlink_ours "$apps/$entry" "$prefix/share/applications/$entry"
done
command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps" || true

# The login item PC > Server turns on, when it starts this install
login="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/io.github.onevr.desktop"
if [ -f "$login" ] && grep -qF "$prefix/bin/onevr" "$login"; then
    rm -f "$login"
fi

# and what an interrupted install.sh left
rm -rf "$prefix" "$prefix.staging" "$prefix.old"
echo "oneVR removed. Your settings stay in $(tilde "${XDG_CONFIG_HOME:-$HOME/.config}/onevr")."
