# Invertonator-rs — Full Source Package (v0.3.0-alpha.16, Linux cooking release)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.16 is (the Fedora release)

You moved to Fedora — this release moves the app to Fedora:

- **NEW backend: GNOME Shell extension (Wayland).** Fedora 43+ ships GNOME
  with the X11 session REMOVED — there is no "GNOME on Xorg" gear option
  anymore — and Mutter silently discards gamma writes from XWayland, so
  the gamma-ramp binary could never work there. The extension applies a
  fragment-shader effect (`1.0 - color`) to the compositor's stage:
  inversion happens INSIDE gnome-shell, strictly after every app (and its
  web content) rendered → websites cannot see it, the same anti-detection
  property as the Windows Magnifier and X11 gamma backends. Super+I
  toggles, a panel indicator shows the state, the state survives reboots,
  binding is reconfigurable via gsettings.
- **The X11 binary gained session awareness:** on a Wayland-only session
  it refuses with actionable guidance (instead of "no X display"), and
  through the XWayland path it warns that the ramp write will be a no-op
  before attempting it. `--self-test` verdicts now point at the extension
  where relevant.
- **CI** now ships `Invertonator-gnome-extension-latest` next to the
  binaries.
- **Windows side: untouched** — alpha.15 code, version bump only. The
  alpha.15 test protocol (YouTube hole stability, hysteresis telemetry)
  remains valid for the exe.

---

## Apply on Fedora (your new daily machine)

```bash
sudo dnf install -y unzip git   # if missing
cd ~/invertonator-push          # your clone; or clone fresh first
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d .
git add -A
git commit -m "alpha.16: GNOME/Wayland shell extension backend (Fedora release)"
git push origin HEAD
```

CI then rebuilds all artifacts for the repo history; on Fedora you can
also skip CI and build the X11 binary locally:

```bash
sudo dnf install -y rust cargo
cargo build --release           # → target/release/invertonator
```

## Use it on Fedora — pick by your session

```bash
echo $XDG_SESSION_TYPE
```

**`wayland` (Fedora default → almost certainly this one):**

```bash
bash shell-extension/install.sh
```

then **log out and log back in once** (GNOME loads new extensions at
login; Alt+F2 `r` does not restart the shell on Wayland). After login:

- a small brightness icon appears in the top-right panel — Invertonator;
- **Super+I** flips inversion; press again to restore;
- the menu item reads "Inversion: ON / OFF".

**`x11` (other spins/desktops):**

```bash
./target/release/invertonator --self-test   # paste the WHOLE report back
./target/release/invertonator               # Ctrl+Alt+I toggle, Ctrl+Alt+Q quit
```

**KDE/Wayland:** KWin ships a built-in Invert effect (Shortcuts → Window
Management → Desktop Effects → "Invert Effect") — works today, no
Invertonator integration yet.

---

## Test protocol — Fedora (5 minutes, Wayland)

1. After logout/login, panel icon present, **Super+I inverts instantly**;
   text is perfectly crisp (compositor shader, not a magnifier hack).
2. Open YouTube and play a video: it renders inverted — **expected** in
   v1 (global inversion); true-color holes are the v2 milestone
   (per-window-actor inversion with a picked un-inverted window).
3. Toggle off/on a few times — no flicker, no stale rectangles.
4. Reboot with inversion ON (toggle it on, then reboot): the session
   comes back inverted.
5. Bind check: `gsettings get org.gnome.shell.extensions.invertonator
   toggle-invert` → `['<Super>i']`.

Report back ONE line per item: invert works, crisp, flicker none, reboot
state, session+version (`echo $XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP; gnome-shell --version`).

If anything is off:

```bash
gnome-extensions info invertonator@phoenix1504e
journalctl --user -b /usr/bin/gnome-shell | grep -i -E 'invertonator|error' | tail -20
```

— paste both outputs.

---

## Windows machine (if you still boot it)

Same flow as every alpha — from PowerShell:

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.16: GNOME/Wayland shell extension backend (Fedora release)"
git push origin HEAD
```

Verify the exe: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must
read `=== Invertonator-rs v0.3.0-alpha.16 ===`. The Windows behavior is
alpha.15-identical (hysteresis fixes) — the alpha.15 test protocol and
the Desktop `invertonator-diagnostics.txt` escalation still apply there.

---

## File map (23 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.16 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix (5 platforms) + roadmap
docs/LINUX-TESTING.md        Fedora-first runbook (Wayland ext / X11 / KDE)
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION
src/win_main.rs              Windows backend (tray, hotkeys, probe, DPI,
                             auto-diagnostics, hysteresis telemetry)
src/holes.rs                 hole pool + main-thread pump (≈64Hz, context-
                             aware evidence, pump heartbeat)
src/tracker.rs               UIA media discovery (cached+live walks, same-
                             name merge, wake loop, fail-open offscreen,
                             sticky trust, ANTI-FLAP HYSTERESIS)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma inversion + --self-test +
                             Wayland session awareness
shell-extension/install.sh                       one-command installer
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v16, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader
                                                           inversion, Super+I,
                                                           panel indicator,
                                                           reboot persistence
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
```

## After testing

Reply with what you saw (one line is enough — e.g. "wayland, super+i
works, crisp, youtube inverted" or "extension failed, journal says …").
The Windows hole saga (YouTube stability) resumes exactly where alpha.15
left it whenever you boot Windows again.
