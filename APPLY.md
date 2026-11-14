# Invertonator-rs — Full Source Package (v0.3.0-alpha.19, snippet-based inversion)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.19 fixes (your latest report)

> Journal: `object class 'ClutterShaderEffect' has no property named
> 'shader-source'`

GNOME 51 gutted BOTH classic ways of handing GLSL to `Clutter.ShaderEffect`
— the `set_shader_source()` method (alpha.17's failure) AND the
`shader-source` property (alpha.18's failure). The official "Port
Extensions to GNOME Shell 51" guide confirms the new contract: GLSL now
comes from a `Cogl.Snippet` returned by `vfunc_get_static_snippet()`
(`Shell.GLSLEffect` was removed in the same sweep).

alpha.19 adopts the full proven pattern — copied from an extension that
ships whole-desktop GLSL effects and is TESTED on GNOME 48 + 51 Wayland:

- dual-API effect class: `Shell.GLSLEffect` (≤49) vs
  `Clutter.ShaderEffect` + snippet (50+), so shell-version 45–51 is
  genuinely covered, not just tolerated;
- effect attached to `Main.layoutManager.uiGroup` (the tree holding
  windows, wallpaper and panel) instead of `global.stage` — the
  whole-desktop target that pattern uses;
- texture sampled via `cogl_sampler0` (auto-provided by Cogl — never
  re-declared);
- enable() journal now prints which effect base was chosen, so the next
  report pins the API in one line.

alpha.18 kept: install.sh honesty lines, zip-freshness gate below.

Windows side: untouched (alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.19`**. If it
prints alpha.18 or older, re-download the zip from the chat first.

## Apply on Fedora — paste this whole block

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.19: snippet-based inversion (GNOME 51 ShaderEffect API) + uiGroup attach" && \
git push origin HEAD && \
bash shell-extension/install.sh
```

(`&&` chaining so a failed step stops the rest; if `git status --short`
ever lists files that are NOT repo files, stop and tell me.)

## Activate — one command + one logout

```bash
bash shell-extension/install.sh
```

Then **log out and log back in**. You are REINSTALLING over a copy the
running shell already executes, and Wayland cannot hot-reload extension
code — the logout guarantees the new module actually runs. (On a
brand-new install with the shell still in ERROR state, a live enable
does re-import — but never gamble; the logout is 20 seconds.)

After login: brightness icon in the top-right panel → **Super+I** flips
the whole screen.

## Test protocol (2 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 19**, State: ACTIVE.
2. Super+I inverts instantly; text stays perfectly crisp.
3. Toggle off/on a few times — no flicker.
4. YouTube plays inverted (expected — global inversion is v1; true-color
   holes are the v2 milestone).
5. Reboot with inversion ON → session comes back inverted.

Report back ONE line: state + invert + crisp + flicker (+ `gnome-shell --version`).

If anything is off:

```bash
journalctl --user -b --no-pager | grep -iE 'invertonator|JS ERROR' | tail -30
```

---

## Windows machine (if you still boot it)

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.19: snippet-based inversion (GNOME 51 ShaderEffect API) + uiGroup attach"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.19 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (23 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.19 (windows+linux deps)
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
shell-extension/install.sh                       installer + gsettings enable fallback
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v19, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           panel indicator
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
```
