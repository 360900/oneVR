#!/bin/sh
# Removes the oneVR that install.sh installed. Settings and pairing are kept.
set -eu

prefix="$HOME/.local/opt/onevr"
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

if [ ! -e "$prefix" ]; then
    echo "uninstall.sh: oneVR isn't installed in $prefix" >&2
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

rm -rf "$prefix"
echo "uninstall.sh: oneVR removed; settings stay in ${XDG_CONFIG_HOME:-$HOME/.config}/onevr"
