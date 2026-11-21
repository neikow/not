#!/usr/bin/env bash
# Packs not.app into a drag-and-drop installer: a disk image whose window shows the app, an
# Applications shortcut and an arrow between them. Stock macOS tools only (hdiutil, Finder, swift).
#
#   scripts/make-dmg.sh <path/to/not.app> <out.dmg>
#   DMG_LAYOUT=0 scripts/make-dmg.sh …   # skip the Finder step (no window layout)
#
# The window layout is set through Finder (AppleScript). Where Finder can't be scripted (no GUI
# session, or no Automation permission), the image is still made, just without the layout.
set -euo pipefail

app="${1:?usage: make-dmg.sh <not.app> <out.dmg>}"
out="${2:?usage: make-dmg.sh <not.app> <out.dmg>}"
here="$(cd "$(dirname "$0")" && pwd)"
volume="not"
# Window and icon spots, in points; dmg-background.swift draws to match.
width=660
height=400
icon=112

test -d "$app" || { echo "no app at $app" >&2; exit 1; }
work="$(mktemp -d)"
mount=""
trap '[ -n "$mount" ] && hdiutil detach -quiet "$mount" 2>/dev/null; rm -rf "$work"' EXIT

# What the window holds.
stage="$work/stage"
mkdir -p "$stage/.background"
ditto "$app" "$stage/not.app"
ln -s /Applications "$stage/Applications"
swift "$here/dmg-background.swift" "$stage/.background/background.png"
# The app's icon on the mounted volume, too.
if [ -f "$app/Contents/Resources/AppIcon.icns" ]; then
    cp "$app/Contents/Resources/AppIcon.icns" "$stage/.VolumeIcon.icns"
fi

# A writable image to lay out, then a compressed read-only one from it.
rw="$work/rw.dmg"
hdiutil create -quiet -volname "$volume" -srcfolder "$stage" -fs HFS+ -format UDRW -ov "$rw"
# Under /Volumes, where Finder sees it; its name there may differ ("not 1") if one is mounted.
mount="$(hdiutil attach -readwrite -noverify -noautoopen "$rw" | grep -o '/Volumes/.*$')"
disk="$(basename "$mount")"

if [ -f "$mount/.VolumeIcon.icns" ] && command -v SetFile >/dev/null; then
    SetFile -a C "$mount"
    SetFile -a V "$mount/.VolumeIcon.icns"
fi

# Finder addresses the disk by its name.
if [ "${DMG_LAYOUT:-1}" = "0" ]; then
    echo "skipping the window layout (DMG_LAYOUT=0)" >&2
elif ! osascript <<APPLESCRIPT
tell application "Finder"
    tell disk "$disk"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set bounds of container window to {200, 120, $((200 + width)), $((120 + height + 28))}
        set options to the icon view options of container window
        set arrangement of options to not arranged
        set icon size of options to $icon
        set text size of options to 13
        set background picture of options to file ".background:background.png"
        set position of item "not.app" of container window to {165, 190}
        set position of item "Applications" of container window to {495, 190}
        update without registering applications
        delay 1
        close
    end tell
end tell
APPLESCRIPT
then
    echo "warning: Finder couldn't lay out the window; the image works, without the arrow" >&2
fi

# Finder writes .DS_Store lazily: give it a moment, then make sure nothing's still writing.
sync
sleep 2
hdiutil detach -quiet "$mount"
mount=""
rm -f "$out"
hdiutil convert -quiet "$rw" -format ULFO -o "$out"
echo "$out"
