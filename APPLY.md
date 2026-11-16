# Invertonator-rs — Full Source Package (v0.3.0-alpha.24, auto-hole fix)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.24 fixes ("auto hole doesnt woks")

Your report was correct — alpha.23's media lock-on had three real bugs.
All three are fixed; the feature is rebuilt on the proven pierce
machinery instead of the broken overlay:

1. **SHIFTED HOLES (the main one).** alpha.23's element rect was a
   `Clutter.Clone` overlay of the window whose translation anchored the
   cloned content at stage (0,0) — that only lines up when the window
   itself sits at (0,0) (fullscreen on the primary monitor). Every
   windowed browser got a *shifted copy* of itself as the "hole".
   alpha.24: the hole is ONE shader per window whose GLSL bakes the
   rect coordinates as literals and inverts ONLY inside them — the same
   double-invert involution as Super+F7, at element granularity. The
   rect math normalizes against the actor's own paint-volume box (the
   exact bounds Cogl's offscreen texture spans), so shadows/CSD margins
   cancel by construction, and the effect rides the window actor:
   moving the window moves its holes for free.
2. **HOLES NEVER FOUND on big pages.** The walk budget (240 D-Bus
   calls ≈ ~120 tree nodes) starved on real web trees — a YouTube page's
   accessibility tree is thousands of nodes. alpha.24 doubles the
   budget to 480 and adds FREE event capture: AT-SPI events carry their
   source element, so media that announces itself (player starts,
   page loads, scroll) is captured with zero walk budget and matched to
   a window by rect containment — even when app matching fails. A
   budget-starved walk no longer wipes holes.
3. **WRONG PRIORITY.** The fullscreen whole-window auto-hole suppressed
   element rects (chrome went true color too). alpha.24: element rects
   BEAT the fullscreen hole — fullscreen YouTube now shows the video
   true color with the chrome still inverted (your v2.2 request), while
   native players and games (no media nodes in their tree) keep the
   whole-window hole as the fallback. Manual Super+F7 holes still beat
   everything.
4. Also: `toolkit-accessibility` is switched ON by the extension itself
   (GTK3 apps need a restart to pick it up; Firefox/Chromium/GTK4
   usually react to the a11y bus alone), and every decision journals
   itself — swaps, fallbacks, walk summaries, coordinate anchors.

alpha.23 kept: hole manager submenu, hole survival, scanout guard,
OR-window pierce, Super+I / Super+F7 keys. Windows side: untouched
(alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.24`**. If it
prints alpha.23 or older, re-download the zip from the chat first.
(Also: only use the block from the LATEST chat message — an old
scrollback apply block just stops harmlessly at "nothing to commit",
but don't rely on that.)

## Apply on Fedora — paste this whole block

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.24: Linux v2.3 — auto-hole fix (rect-shader element holes, event capture, element rects beat fullscreen auto-hole)" && \
git push origin HEAD && \
bash shell-extension/install.sh
```

(`&&` chaining so a failed step stops the rest; if `git status --short`
ever lists files that are NOT repo files, stop and tell me.)

## Activate — one command + one logout

```bash
bash shell-extension/install.sh
```

Then **log out and log back in**. Reinstalling over a copy the running
shell already executes cannot hot-reload — the logout guarantees the
new module actually runs.

After login: install.sh output should include FIVE
`schema check: '...' present` lines (toggle-invert, toggle-pierce,
auto-hole-media, auto-hole-fullscreen, start-enabled).

## Test protocol (6 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 24**, State: ACTIVE.
2. `journalctl --user -b --no-pager | grep -i Invertonator | tail -15` →
   expect: both `registered` keybinding lines and `AT-SPI2 ready — media
   lock-on roles: image,video,animation` (or the Atspi-import warning —
   paste either). First run may also log the
   `toolkit-accessibility was OFF — set ON` line; apps that were already
   open need a restart once to expose their trees.
3. Super+I → inverted. Windowed Firefox on YouTube (or any page with a
   big image): within a few seconds a true-color RECT appears at exactly
   the video/image — text and chrome stay inverted. Panel menu shows
   `media lock-on: N rects (auto)`.
4. **The alpha.23 killer test:** resize the window or move it — the rect
   must stay glued to the video (the hole rides the window actor now).
   In alpha.23 a moved window showed a shifted copy here.
5. Scroll → rects follow the elements (may lag ~150ms; that is the
   refresh throttle). Switch tab → rects re-lock to the new tab's media.
   Switch focus to a terminal → rects clear, terminal stays fully
   inverted.
6. Fullscreen the video: video stays true color AND the browser chrome
   goes back to INVERTED (element rects beat the whole-window hole — the
   journal prints `media rects (N) beat fullscreen hole`). Exit
   fullscreen → rects keep following.
7. mpv / VLC / a game fullscreen → whole window true color (fallback
   hole, journal: `auto-hole ON (fullscreen, no media nodes)`).

Report back ONE line: version + whether windowed video/images got true-
color rects at the right place + whether fullscreen showed video true
with chrome inverted + the journal tail if anything is off:

```bash
journalctl --user -b --no-pager | grep -iE 'invertonator|JS ERROR' | tail -40
```

---

## Windows machine (if you still boot it)

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.24: Linux v2.3 — auto-hole fix (rect-shader element holes, event capture, element rects beat fullscreen auto-hole)"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.24 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (24 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.24 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix (5 platforms) + roadmap
docs/LINUX-TESTING.md        Fedora-first runbook (Wayland ext / X11 / KDE) +
                             holes test protocol + journal-line diagnosis
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION (alpha.24)
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
shell-extension/install.sh                       installer + schema verification + gsettings fallback
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v24, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           Super+F7 pierce holes, hole
                                                           manager submenu, fullscreen
                                                           auto-hole fallback, AT-SPI2
                                                           media lock-on v2.3 (per-window
                                                           rect-shader holes, event
                                                           capture), scanout guard
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
