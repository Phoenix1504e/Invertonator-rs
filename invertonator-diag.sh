#!/usr/bin/env bash
# Invertonator-rs — Linux auto-hole diagnostics (one shot, no root, no net).
#
# Collects everything needed to debug "media lock-on finds nothing":
#   1. running vs installed extension version
#   2. session / shell info
#   3. toolkit-accessibility gsetting
#   4. browser accessibility prefs (LibreWolf / Firefox, native + flatpak)
#   5. AT-SPI2 tree probe — what ANY client on the a11y bus can see:
#      per-app document-node + IMAGE/VIDEO/ANIMATION counts and sample
#      screen extents (the same discovery the extension performs)
#   6. journal tail (Invertonator + JS ERROR)
#
# Run it WHILE the video page is open (inversion need not be on).
# Output: paste the ENTIRE output back into the chat.

set -u
shopt -s nullglob

sec() { echo; echo "======================================================"; echo "== $1"; echo "======================================================"; }

sec "1. Extension: running vs installed"
gnome-extensions info invertonator@phoenix1504e 2>&1 | sed 's/^/  /'
MD="$HOME/.local/share/gnome-shell/extensions/invertonator@phoenix1504e/metadata.json"
if [ -f "$MD" ]; then
    python3 -c "import json;print('  installed (on disk) version:', json.load(open('$MD'))['version'])" 2>/dev/null \
        || echo "  installed metadata unreadable: $MD"
else
    echo "  NOT INSTALLED at ~/.local/share/gnome-shell/extensions/"
fi

sec "2. Session"
echo "  XDG_SESSION_TYPE=$XDG_SESSION_TYPE  XDG_CURRENT_DESKTOP=$XDG_CURRENT_DESKTOP"
command -v gnome-shell >/dev/null && echo "  $(gnome-shell --version 2>/dev/null)"

sec "3. a11y settings"
echo "  toolkit-accessibility: $(gsettings get org.gnome.desktop.interface toolkit-accessibility 2>&1)"

sec "4. Browser accessibility prefs"
echo "  (accessibility.force_disabled = 1 means the browser REFUSES to expose its tree)"
for f in "$HOME"/.librewolf/*/prefs.js \
         "$HOME"/.var/app/io.gitlab.librewolf-community/.librewolf/*/prefs.js \
         "$HOME"/.mozilla/firefox/*/prefs.js \
         /usr/lib64/librewolf/librewolf.cfg /usr/lib/librewolf/librewolf.cfg \
         /usr/lib64/librewolf/defaults/pref/*.js /usr/lib/librewolf/defaults/pref/*.js; do
    [ -f "$f" ] || continue
    hits=$(grep -E 'accessibility\.force_disabled|resistFingerprinting' "$f" 2>/dev/null)
    echo "  -- $f"
    if [ -n "$hits" ]; then
        echo "$hits" | sed 's/^/       /'
    else
        echo "       (no accessibility/RFP prefs set here)"
    fi
done
if [ -z "$(ls -d "$HOME"/.librewolf "$HOME"/.mozilla/firefox "$HOME"/.var/app/io.gitlab.librewolf-community 2>/dev/null)" ]; then
    echo "  (no LibreWolf/Firefox profile dirs found)"
fi

sec "5. AT-SPI2 tree probe (what ANY client sees)"
echo "  This walks every app's a11y tree like the extension does."
echo "  It may take 10-40s; Ctrl+C aborts safely."
python3 - "$@" <<'PY' 2>&1 | sed 's/^/  /'
import sys

try:
    import gi
    gi.require_version('Atspi', '2.0')
    from gi.repository import Atspi
except Exception as e:
    print(f"AT-SPI bindings unavailable: {e}")
    print("(on Fedora: python3-gobject + at-spi2-core are stock; report this)")
    sys.exit(0)

from collections import deque

MEDIA = {}
for name in ('IMAGE', 'VIDEO', 'ANIMATION'):
    try:
        MEDIA[getattr(Atspi.Role, name)] = name.lower()
    except Exception:
        pass

try:
    desktop = Atspi.get_desktop(0)
except Exception as e:
    print(f"a11y bus UNREACHABLE: {e}")
    sys.exit(0)

def role_name(value):
    try:
        return Atspi.Role(value).name
    except Exception:
        return str(value)

def walk(root, max_nodes=3000):
    coord = Atspi.CoordType.SCREEN
    counts, samples, docs = {}, [], 0
    q = deque([root])
    visited = 0
    while q and visited < max_nodes:
        node = q.popleft()
        visited += 1
        try:
            role = node.get_role()
        except Exception:
            continue
        if role in MEDIA:
            counts[role] = counts.get(role, 0) + 1
            if len(samples) < 4:
                try:
                    ext = node.get_component().get_extents(coord)
                    samples.append((MEDIA[role], ext.x, ext.y, ext.width, ext.height))
                except Exception:
                    samples.append((MEDIA[role], 'extents-unreadable'))
        nm = role_name(role)
        if nm.startswith('DOCUMENT'):
            docs += 1
        try:
            cnt = node.get_child_count()
        except Exception:
            continue
        for i in range(min(cnt, 80)):
            try:
                k = node.get_child_at_index(i)
                if k is not None:
                    q.append(k)
            except Exception:
                pass
    return visited, docs, counts, samples

try:
    n = desktop.get_child_count()
except Exception as e:
    print(f"desktop unreadable: {e}")
    sys.exit(0)

print(f"a11y bus reachable — {n} app(s) registered")
any_media = False
for i in range(min(n, 25)):
    try:
        app = desktop.get_child_at_index(i)
        name = app.get_name() or '?'
        try:
            pid = app.get_process_id()
        except Exception:
            pid = -1
    except Exception as e:
        print(f"app[{i}]: unreadable ({e})")
        continue
    try:
        visited, docs, counts, samples = walk(app)
    except Exception as e:
        print(f"app[{i}] {name!r} pid={pid}: walk failed ({e})")
        continue
    if counts:
        any_media = True
    cstr = ', '.join(f"{MEDIA[r]}x{c}" for r, c in counts.items()) or 'NONE'
    print(f"app[{i}] {name!r} pid={pid}: nodes={visited} document-nodes={docs} media={cstr}")
    for s in samples:
        print(f"      sample: {s[0]} rect=({s[1]},{s[2]}) {s[3]}x{s[4]}" if len(s) == 5
              else f"      sample: {s[0]} ({s[1]})")

print()
if not any_media:
    print("VERDICT: NO app exposes IMAGE/VIDEO nodes on the a11y bus.")
    print("-> The extension is not the problem: there is nothing to find.")
    print("-> If LibreWolf/Firefox shows document-nodes=0 or is missing above,")
    print("   its accessibility is disabled — set accessibility.force_disabled=0")
    print("   in about:config and restart the browser (LibreWolf ships it =1).")
    print("-> If a probe HUNG on one app, that app is a11y-sick; note which one.")
else:
    print("VERDICT: media roles ARE exposed to clients.")
    print("-> If the extension still finds nothing, the failure is in matching/")
    print("   walking — paste this output together with the journal tail below.")
PY

sec "6. Journal tail (this boot)"
journalctl --user -b --no-pager 2>/dev/null | grep -iE 'invertonator|JS ERROR' | tail -30 | sed 's/^/  /'

echo
echo "======================================================"
echo ">>> PASTE THIS ENTIRE OUTPUT BACK INTO THE CHAT <<<"
echo "======================================================"
