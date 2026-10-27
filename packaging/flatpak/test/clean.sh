#!/usr/bin/env bash
# clean.sh — remove the test Flatpak build.sh installs: the app itself and
# its own data folder. Run after every measurement session; build.sh
# itself refuses to run again while either is still present.

set -euo pipefail

APP_ID="io.github.nabbisen.orbok"

if flatpak --user info "$APP_ID" >/dev/null 2>&1; then
    flatpak --user uninstall -y "$APP_ID"
else
    echo "clean.sh: $APP_ID is not installed (user) -- nothing to uninstall"
fi

app_data_dir="$HOME/.var/app/$APP_ID"
if [ -e "$app_data_dir" ]; then
    rm -rf "$app_data_dir"
    echo "clean.sh: removed $app_data_dir"
else
    echo "clean.sh: $app_data_dir does not exist -- nothing to remove"
fi
