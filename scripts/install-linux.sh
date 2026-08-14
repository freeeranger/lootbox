#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIRECTORY="$(cd -- "${SCRIPT_DIRECTORY}/.." && pwd)"
BINARY="${PROJECT_DIRECTORY}/src-tauri/target/release/lootbox"
ICON="${PROJECT_DIRECTORY}/src-tauri/icons/icon.svg"
DESKTOP_FILE="${PROJECT_DIRECTORY}/packaging/linux/lootbox.desktop"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run this installer with sudo." >&2
  exit 1
fi

if [[ ! -x "${BINARY}" ]]; then
  echo "Release binary not found. Run: npm run tauri build -- --no-bundle" >&2
  exit 1
fi

install -Dm755 "${BINARY}" /usr/bin/lootbox
install -Dm644 "${ICON}" /usr/share/icons/hicolor/scalable/apps/lootbox.svg
install -Dm644 "${DESKTOP_FILE}" /usr/share/applications/lootbox.desktop

# Remove files written by the earlier reverse-domain launcher. KWin reports this
# application's actual Wayland desktop file name as "lootbox".
for obsolete_file in \
  /usr/share/applications/com.lootbox.desktop.desktop \
  /usr/share/icons/hicolor/scalable/apps/com.lootbox.desktop.svg
do
  if [[ -L "${obsolete_file}" ]]; then
    echo "Refusing to remove unexpected symbolic link: ${obsolete_file}" >&2
    exit 1
  fi
  if [[ -f "${obsolete_file}" ]]; then
    rm -- "${obsolete_file}"
  fi
done

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null
fi

echo "Lootbox installed. Refresh KDE's application cache with: kbuildsycoca6"
