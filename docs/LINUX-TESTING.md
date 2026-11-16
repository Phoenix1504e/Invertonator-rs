# Testing Invertonator on Linux — a 10-minute runbook

Invertonator has TWO Linux backends. Pick by your session type:

| Your `$XDG_SESSION_TYPE` | Backend | This runbook |
|---|---|---|
| `wayland` (Fedora default!) | **GNOME Shell extension** (`shell-extension/`) | Part 1 |
| `x11` | Rust binary — XRandR gamma ramp | Part 2 |

Check yours with:

```bash
echo $XDG_SESSION_TYPE
```

**Fedora note (important):** Fedora 43+ ships GNOME with the X11 session
REMOVED — there is no "GNOME on Xorg" gear option at the login screen
anymore. If you installed Fedora Workstation recently, you are on Wayland
and Part 1 is the only backend that works. (KDE spin users: see Part 3.)

---

## Part 1 — Wayland (GNOME): the Shell extension

Nothing to build, nothing to compile, no Rust required — the extension is
plain JavaScript that runs inside gnome-shell itself. It inverts every
pixel in the compositor, AFTER all apps (and their web content) rendered,
so the web layer cannot see it — the same anti-detection property as the
Windows Magnifier backend and the X11 gamma ramp.

### Get the extension

Either from the repo you already have:

```bash
bash shell-extension/install.sh
```

or from CI (no git needed): GitHub → **Actions** → latest **Build** →
download artifact **`Invertonator-gnome-extension-latest`**, unzip it, then
inside the unpacked folder:

```bash
unzip invertonator-gnome-extension.zip
bash install.sh
```

### Activate

1. **First install on Wayland: log out and log back in once.** GNOME only
   loads newly installed extensions at login (Alt+F2 `r` does NOT restart
   the shell on Wayland).
2. After logging in you will see a small brightness icon in the top-right
   panel — that is Invertonator.
3. Press **Super+I**. The whole screen snaps to inverted. Press again to
   restore. The panel menu item reads "Inversion: ON / OFF".

Why Super+I and not F5? A shell-level grab of a plain F-key would shadow
the browser's refresh shortcut system-wide. You can rebind:

```bash
gsettings set org.gnome.shell.extensions.invertonator toggle-invert "['<Alt>i']"
```

The extension remembers its state across reboots (toggling writes
`start-enabled`, which is re-applied at login).

### Holes and auto-holes (v2)

1. Invert with **Super+I**, hover a window and press **Super+F7** — that
   window snaps to true color while the rest of the screen stays inverted.
   Press Super+F7 over it again to close the hole. The panel menu's
   "Holes" submenu lists every hole (click a row to close it).
2. Windowed YouTube (or any page with a photo): the video/image element
   should come up true color ON ITS OWN within a few seconds while the
   page around it stays inverted — that is media lock-on
   (`auto-hole-media`, on by default). The journal shows the decisions.
3. Fullscreen YouTube: the video stays true color AND the chrome goes
   back to inverted — element rects beat the whole-window hole. A native
   player or a game (no a11y media nodes) gets the whole-window hole
   instead — true color everywhere, also automatic.
4. Turning inversion off (Super+I) clears every hole and rect.

### Media lock-on journal lines (paste these when reporting)

```bash
journalctl --user -b /usr/bin/gnome-shell | grep Invertonator | tail -40
```

The lines that decide auto holes:

- `AT-SPI2 ready — media lock-on roles: image,video,animation` — the
  a11y import worked. Its absence means no element holes ever.
- `toolkit-accessibility was OFF — set ON` — the extension enabled it;
  apps already running needed a restart to expose their trees.
- `no AT-SPI app for pid=… class="…"` — the focused window's app could
  not be matched (event capture still applies).
- `a11y walk "app" calls=N media=K cache=…` — one per interesting walk;
  `media=0` on a page you KNOW has images/videos is the number to report.
- `media lock-on N rect(s) on M window(s) (kinds)` — rects published.
- `rect bake "class" actor@(x,y) box=(…)` — the coordinate anchor of the
  first bake per app; if holes appear SHIFTED, this + a screenshot
  pinpoints the mapping bug.
- `media rects (N) beat fullscreen hole — chrome re-inverts on "…"` and
  `auto-hole ON (fullscreen, no media nodes) "…"` — the priority swap
  and the native-player fallback doing their jobs.

### Verify + report back

Send me one line per item:

1. Did the screen invert on Super+I? (yes / no)
2. Text crispness — this is a compositor shader, text should be perfectly
   crisp. (crisp / blurry)
3. Any flicker when the effect toggles? (none / flicker on…)
4. Does YouTube/video render inverted? (expected: YES — global inversion
   is v1; per-window true-color "holes" are the v2 milestone, tracked in
   the repo roadmap)
5. Session + version: `echo $XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP` and
   `gnome-shell --version`.

### Troubleshooting

- **No icon in the panel, Super+I dead** — the extension did not load.
  Check: `gnome-extensions info invertonator@phoenix1504e`. If it says
  ERROR, send me the output of
  `journalctl --user -b /usr/bin/gnome-shell | grep -i -E 'invertonator|error'`.
- **"Extension version must be 45–51"** — `metadata.json` lists
  shell-version 45–51; if your GNOME is older/newer, edit that array to
  include your version and re-run install.sh.
- **HDR enabled?** Compositor effects assume SDR; if colors look wrong,
  disable HDR first.
- **Uninstall**: `gnome-extensions disable invertonator@phoenix1504e &&
  rm -rf ~/.local/share/gnome-shell/extensions/invertonator@phoenix1504e`

---

## Part 2 — X11 session: the gamma-ramp binary

Works on any desktop running an X11 session (KDE on X11, XFCE, Mint,
Cinnamon, i3 …). Inversion happens in the display hardware's gamma LUT —
below every application.

### Step 1 — get the binary

GitHub → **Actions** → latest **Build** → artifact
**`Invertonator-rs-linux-x64-latest`**:

```bash
unzip Invertonator-rs-linux-x64-latest.zip
chmod +x invertonator
```

(Or build from source: `cargo build --release`.)

### Step 2 — the one-command diagnostic

```bash
./invertonator --self-test
```

Prints a full paste-back report: session type, X server identity, every
output with its gamma hardware, a live invert/restore round-trip (catches
drivers that silently ignore gamma changes), and hotkey availability.
Exit code `0` = machine supports gamma inversion, `1` = real failure,
`2` = environment unavailable. **Send back the ENTIRE output.**

### Step 3 — the visual test

```bash
./invertonator
```

1. Screen inverts **immediately** on launch.
2. Any browser, any desktop — text stays crisp (hardware LUT, not a
   magnifier hack).
3. Images/videos invert too — expected in v1; true-color media holes are
   the v2 XComposite milestone on Linux.
4. `Ctrl+Alt+I` → toggle. `Ctrl+Alt+Q` → restore + exit.
5. Multi-monitor: every connected monitor inverts.

### Step 4 — crash-recovery drill

```bash
./invertonator &          # start inverted
pkill -9 invertonator     # simulate a crash — ramp PERSISTS (expected)
./invertonator --restore  # identity ramps — colors come back
```

---

## Part 3 — KDE on Wayland

KWin ships its own compositor-level **Invert** effect (the same
post-render inversion property): System Settings → Keyboard → Shortcuts →
Window Management → Desktop Effects → "Invert Effect", bind a key there.
It does not integrate with Invertonator's hotkeys/logs, but it works
today. A dedicated KWin script backend remains on the roadmap.

## What to send back (either backend)

- Backend + session: `echo $XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP`,
  `gnome-shell --version` (Wayland) or the full `--self-test` report (X11).
- GPU: `lspci | grep -iE 'vga|3d'`.
- One line on: inversion works, text crispness, flicker, all monitors.

## No Linux box? (maintainer note)

- CI builds the Linux binary on every push; any Linux tester can follow
  Parts 2–3. The GNOME extension can be smoke-tested in a GNOME Boxes /
  VirtualBox VM running Fedora Workstation (Wayland by default — Part 1).
- WSLg is **not** representative: it is Wayland/XWayland and intercepts
  CRTC gamma writes — the self-test will correctly report RAMP-UNCHANGED.
