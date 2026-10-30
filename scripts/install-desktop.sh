#!/usr/bin/env bash
# Install the GUI and launcher for the current user, without root privileges.
set -euo pipefail

pomni_root=$(dirname -- "$(dirname -- "$(realpath -- "${BASH_SOURCE[0]}")")")
pomni_binary="$pomni_root/target/release/pomni-gui"
# The same installer works in a checkout and in an extracted release package.
if [[ ! -f "$pomni_binary" && -f "$pomni_root/pomni-gui" ]]; then
    pomni_binary="$pomni_root/pomni-gui"
fi
if [[ ! -f "$pomni_binary" ]]; then
    printf '%s\n' 'Compile primeiro: cargo build --release --locked --features gui --bin pomni-gui' >&2
    exit 1
fi
: "${HOME:?A variável HOME precisa estar definida.}"
case "${XDG_DATA_HOME:-}" in
    /*) pomni_data=$XDG_DATA_HOME ;;
    *) pomni_data="$HOME/.local/share" ;;
esac
pomni_bin="$HOME/.local/bin"
pomni_apps="$pomni_data/applications"
mkdir -p -- "$pomni_bin" "$pomni_apps"
pomni_executable="$pomni_bin/pomni-gui"
pomni_temporary=$(mktemp "$pomni_bin/.pomni-gui.XXXXXXXX")
pomni_launcher_temporary=''
cleanup() {
    [[ -z "$pomni_temporary" ]] || rm -f -- "$pomni_temporary"
    [[ -z "$pomni_launcher_temporary" ]] || rm -f -- "$pomni_launcher_temporary"
}
trap cleanup EXIT
# Atomic replacement also works while a previous version is running.
install -m755 -- "$pomni_binary" "$pomni_temporary"
mv -f -- "$pomni_temporary" "$pomni_executable"
install -Dm644 -- "$pomni_root/assets/pomni.svg" "$pomni_data/icons/hicolor/scalable/apps/pomni.svg"
install -Dm644 -- "$pomni_root/assets/pomni.png" "$pomni_data/icons/hicolor/256x256/apps/pomni.png"
# Desktop Exec uses quoted arguments, not a shell command.
pomni_escaped=$(printf '%s' "$pomni_executable" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/\$/\\$/g' -e 's/%/%%/g' -e 's/\x60/\\\x60/g')
pomni_launcher_temporary=$(mktemp "$pomni_apps/.pomni.XXXXXXXX.desktop")
while IFS= read -r pomni_line || [[ -n "$pomni_line" ]]; do
    case "$pomni_line" in
        Exec=*) printf 'Exec="%s"\n' "$pomni_escaped" ;;
        *) printf '%s\n' "$pomni_line" ;;
    esac
done < "$pomni_root/assets/pomni.desktop" > "$pomni_launcher_temporary"
chmod 644 -- "$pomni_launcher_temporary"
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$pomni_launcher_temporary"
fi
mv -f -- "$pomni_launcher_temporary" "$pomni_apps/pomni.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$pomni_apps"
fi
printf 'Aplicativo: %s\nAtalho: %s\n' "$pomni_executable" "$pomni_apps/pomni.desktop"
