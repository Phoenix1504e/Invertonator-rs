# Invertonator-rs — Full Source Package (v0.3.0-alpha.28, adjust + confirm selection)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## STOP — two machines, two blocks. Paste the RIGHT one.

| Terminal prompt | Use |
|---|---|
| `phoenix1504e@fedora:…$` (bash) | the **Fedora one-liner** below — `bash apply-linux.sh` |
| `PS C:\Users\admin\…>` (PowerShell) | the **Windows block** at the very bottom of this file |

**Round 14 field event (this happened AGAIN):** the Windows block was
pasted into Fedora bash — `cd C:\Users\admin\invertonator-push` →
`No such file or directory`, `Expand-Archive: command not found`, then
`nothing to commit, working tree clean` / `Everything up-to-date`.
Diagnosis: **nothing was applied and nothing was harmed** — bash has no
Expand-Archive so the zip was never extracted, and git correctly refused
to commit a clean tree (the alpha.6-era message in that paste was never
committed). Remedy: run the Fedora one-liner below.

New in alpha.28: the Fedora path is **one guarded command**
(`apply-linux.sh` — refuses the wrong machine, refuses a stale zip,
refuses a dirty repo, commits the right message) and the Windows block
is **bash-fatal by design**: its whole flow sits on ONE line behind a
PowerShell `$env:OS` check — bash rejects that line at the `{` token
(verified: zero commands execute), PowerShell on Windows runs it.

---

## What alpha.28 adds (your request: "after selection give them a chance to readjust and then confirm the area")

The alpha.27 selection committed the instant you released the mouse —
too fast to fix a sloppy drag. The prompt is now TWO STEPS:

1. **Step 1 — drag (unchanged feel).** Screen dims, drag a rectangle.
2. **Step 2 — the readjust preview.** On release the dashed rect becomes
   a solid preview and NOTHING is stored yet. You get:
   - **8 resize handles** (4 corners + 4 edges) — drag any of them to
     fine-tune the area, with live `W × H px` size readout;
   - **move** — drag inside the rect to reposition it;
   - **redraw** — drag outside the rect (or the Redraw button, or `R`)
     starts a fresh selection; a too-small outside drag keeps the old
     rect instead of losing your work;
   - **a Confirm / Redraw / Cancel bar** that follows the rect (below it,
     above it when there is no room) — or just press **Enter**.
   Esc cancels everything, right-click cancels, mouse cursors change per
   handle so you always know which way a drag will pull.
3. **Nothing is stored until Confirm** (button or Enter). The commit
   math (rel-to-frame, title tokens) is byte-for-byte alpha.27's — only
   the gating in front of it is new.

## What alpha.27 adds (your request: "select an area once and it gets stored for YouTube")

The auto-hole walker stays exactly as alpha.26 left it (your call: "this is
probably fine"). On TOP of it there is now a manual path that needs NO
discovery at all — which is the only thing that can work where the
accessibility tree is closed (LibreWolf hardened builds):

1. **Super+F8 — the selection prompt.** The screen dims, you drag a
   rectangle over the area that should stay true color (the YouTube
   player), Esc cancels. On release the area is STORED.
2. **Stored, not absolute.** The rectangle is saved normalized to the
   focused window's frame (rel 0..1, four decimals) — so it follows window
   moves and resizes and matches other windows of the same size class, on
   any monitor. Stored in gsettings (`pinned-holes`) — it survives
   reboots and re-logins.
3. **Matched by title tokens.** The anchor window's title is split on its
   separators and stable segments are kept as match tokens:
   `Song X - YouTube — Mozilla Firefox` stores `["song x", "youtube"]`.
   ANY window whose title contains ANY token gets the pin — your NEXT
   video still matches via `- youtube`, video titles change, the site
   name does not. If the title carries nothing stable, the window class
   is stored instead.
4. **Applied automatically.** A cheap 2-second scan (plus focus-change
   and window-created hooks — pure string matching, zero D-Bus) re-bakes
   the stored rect as a true-color hole on every matching visible window
   the moment it appears. Instant, and immune to browsers that hide
   their a11y tree.
5. **Fullscreen-safe.** A matching window that goes FULLSCREEN drops its
   pin (a windowed player's position is meaningless fullscreen) and the
   existing fullscreen auto-hole / media rects take over; leaving
   fullscreen restores the pin.
6. **Priority: manual Super+F7 pierce > stored pins > media rects >
   fullscreen auto-hole.** Media rects never fight your stored area.
7. **Management.** Panel menu: "Pin a stored area", a "Stored areas: N"
   submenu (each row shows its tokens + coordinates; click to forget
   that one) and "Forget all stored areas". Re-storing the exact same
   area replaces it instead of duplicating. Cap: 16 pins (oldest
   evicted).

Quick how-to: Super+I → open YouTube → Super+F8 → drag a box around the
player → adjust the handles until the readout covers the player →
Confirm (or Enter). From then on every YouTube-titled window gets that
hole instantly.

## What alpha.26 fixed (field round 11 — journal proved the thrash)

Your journal finally landed the diagnosis. It showed alpha.25 running,
event capture alive (via the `EventListener.register` fallback — your
AT-SPI build has no static `add_callback`), the carry-over working —
and `media=0` across six 480-call walks on a YouTube page, with the
carried queue reset to 0 after the first tick:

1. **RE-ROOT THRASH (the killer).** YouTube emits AT-SPI
   `children-changed` events continuously while playing (progress bar,
   buffering, DOM churn). alpha.25 treated every one as "the tree
   changed, restart the walk" — so the BFS restarted from the browser
   chrome every tick and NEVER reached the content document where the
   video and thumbnails live. alpha.26: only page loads
   (`document:load-complete/stopped`) re-root; `children-changed` merely
   accelerates the schedule, and the carried queue converges until it
   drains.
2. **STEADY-STATE GUARD.** With the queue drained and no page load, the
   walker no longer re-roots every 3s — it does a cheap extents refresh
   to keep holes glued and only revalidates the tree once per 8s.
3. **DOCUMENT PRIORITY.** The BFS now dives into `DOCUMENT_*` subtrees
   before everything else (sibling order kept), so the walk enters the
   content document as soon as it appears instead of drowning in
   toolbar/bookmark/sidebar nodes.
4. **BETTER FIELD LINES.** The `a11y walk converging` line now prints
   the app name and cache `hits=`, so the next report pins the state in
   one glance.
5. **NEW TOOL: `invertonator-diag.sh`.** One-shot host-side diagnostic
   (no root, no net): running-vs-installed version, toolkit-accessibility,
   LibreWolf/Firefox accessibility prefs (LibreWolf ships
   `accessibility.force_disabled=1` — hardened forks refuse to expose
   their tree to ANY client), an independent AT-SPI probe that walks
   every app and counts IMAGE/VIDEO nodes with sample extents, and the
   journal tail. Run it while the video page is open; paste the output.

## What alpha.25 fixed ("still the holes are not automatic")

alpha.24 rebuilt the element holes on the rect-shader (alpha.23's shifted
overlay is gone). alpha.25 fixes the remaining reason holes could still
NEVER appear, and hardens the AT-SPI surface:

1. **WALKER CONVERGENCE (the main one).** alpha.24 restarted the tree
   walk from the top every 3 seconds. If the walk starved on a big page
   (a YouTube accessibility tree is thousands of nodes) and the
   cross-walk cache missed — GI object wrappers are not guaranteed to
   survive D-Bus re-fetches — the SAME first ~480 nodes were explored
   forever and a video deeper in the tree was never reached.
   alpha.25: the walk queue carries over between ticks — every 3s tick
   spends its 480-call budget on NEW nodes and the walk drains in a few
   ticks, then the holes publish. Progress journals itself
   (`a11y walk converging — queue=N media=M`).
2. **STABLE CACHE KEYS.** The walker cache now probes for the node's
   object path (stable across re-fetches) and journals
   `key=path|wrap`; wrapper identity is only the fallback (and the
   carry-over guarantees convergence either way).
3. **STALE-QUEUE GUARD.** Page loads / DOM changes force the next walk
   to re-root (a carried queue must never keep walking the old page).
4. **EVENT-API FALLBACK.** Event capture registration tries
   `EventListener.add_callback` then `EventListener.new` + `register`,
   and journals which one ran (`event capture on via … (N/5
   listeners)`) — a silently dead capture path is now impossible to
   miss in the journal.
5. **BAKE DIAG.** If every discovered rect clips out during the shader
   bake (paint-volume mismatch signature), the journal says so with the
   exact box coordinates.

## What alpha.24 already fixed ("auto hole doesnt woks")

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

It must print **`Invertonator-rs <sha> 0.3.0-alpha.28`**. If it
prints alpha.27 or older, re-download the zip from the chat first.

> Rounds 10 AND 14 lost to the same paste mix-up (Windows block into
> Fedora bash) — hence the guards below. The full story is in "STOP —
> two machines" at the top.

## Apply on Fedora — paste this ONE command

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip apply-linux.sh -d . && \
bash apply-linux.sh
```

`apply-linux.sh` does the whole flow WITH guards: refuses non-Linux,
verifies the zip is alpha.28 (metadata version + adjust/confirm markers)
BEFORE touching the repo, refuses a dirty repo, resets to `origin/main`,
extracts, commits `alpha.28: Linux v2.3.4 — pin selection adjust +
confirm step (handles, move, Enter/R/Esc)`, pushes, runs the installer
and prints the logout reminder. Zip somewhere else? `bash apply-linux.sh
/path/to/zip`.

Manual fallback (only if you want to do it by hand — NO guards):

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.28: Linux v2.3.4 — pin selection adjust + confirm step (handles, move, Enter/R/Esc)" && \
git push origin HEAD && \
bash shell-extension/install.sh
```

(`&&` chaining so a failed step stops the rest; if `git status --short`
ever lists files that are NOT repo files, stop and tell me. After the
unzip, `git status --short` MUST list changed files — if it prints
nothing, the unzip found nothing new and the gate above was skipped.)

## Activate — one command + one logout

```bash
bash shell-extension/install.sh
```

Then **log out and log back in**. Reinstalling over a copy the running
shell already executes cannot hot-reload — the logout guarantees the
new module actually runs.

After login: install.sh output should include SIX
`schema check: '...' present` lines (toggle-invert, toggle-pierce,
pin-hole, auto-hole-media, auto-hole-fullscreen, start-enabled).

## Test protocol (6 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 27**, State: ACTIVE.
2. `journalctl --user -b --no-pager | grep -i Invertonator | tail -15` →
   expect: THREE `registered` keybinding lines (toggle-invert,
   toggle-pierce, **pin-hole**), `AT-SPI2 ready — media
   lock-on roles: image,video,animation`, and `event capture on via …
   (5/5 listeners)` (or the Atspi-import warning — paste either).
   First run may also log the
   `toolkit-accessibility was OFF — set ON` line; apps that were already
   open need a restart once to expose their trees.
3. Super+I → inverted. Windowed Firefox on YouTube (or any page with a
   big image): within a few seconds (huge pages: up to ~30s while the
   walk converges — watch the `a11y walk converging` lines) a true-color
   RECT appears at exactly
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
8. **NEW — the alpha.28 test (adjust + confirm, then the YouTube pin):**
   inverted, on a YouTube watch page (windowed), press **Super+F8** →
   screen dims with a hint label. Drag a box around the video player and
   RELEASE — a solid preview appears with 8 handles, a `W × H px` readout
   and a Confirm / Redraw / Cancel bar (journal:
   `area preview — drag the handles…`). Drag an edge handle to fine-tune,
   drag inside the rect to move it — journal stays silent (nothing
   stored yet). Press **Enter** (or click Confirm). Journal:
   `area stored — match title tokens [song x, youtube] rel=[…]`.
   Press Super+F8 again, drag, then **R** or Redraw → fresh selection;
   Esc mid-adjust → nothing stored. Close the tab, open ANOTHER video on
   YouTube → within ≤2s a true-color rect appears at the SAME relative
   position (journal: `stored area applied (title […])`) — no walker
   needed. Resize the window → the rect follows. Fullscreen the video →
   the pin drops (`stored area dropped (window went fullscreen)`) and
   normal fullscreen behavior returns; leave fullscreen → pin comes
   back. Panel menu → "Stored areas" lists the pin; click the row to
   forget it.
9. If AT-SPI discovery ALSO still works on your setup, both can coexist:
   the stored pin wins on its window (chrome of a pinned YouTube window
   stays inverted even if the walker is dead — the whole point).

Report back ONE line: version + whether windowed video/images got true-
color rects at the right place + whether fullscreen showed video true
with chrome inverted + the journal tail if anything is off:

```bash
journalctl --user -b --no-pager | grep -iE 'invertonator|JS ERROR' | tail -40
```

---

## Windows machine (if you still boot it) — NEVER paste this into Fedora bash

The whole flow is ONE PowerShell line gated on `$env:OS`: on Windows it
runs; in Fedora bash it is a PARSE ERROR at the `{` token — bash
discards the entire line and RUNS NOTHING. (Verified round 14.) alpha.28
is Linux-only; this section only bumps the Windows VERSION string.

```powershell
if ($env:OS -ne 'Windows_NT') { throw 'WINDOWS PowerShell block — wrong machine.' } else { cd C:\Users\admin\invertonator-push; Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force; git add -A; git commit -m "alpha.28: Linux v2.3.4 — pin selection adjust + confirm step (handles, move, Enter/R/Esc)"; git push origin HEAD }
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.28 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (25 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
apply-linux.sh               ONE-command guarded Fedora apply (verify zip, commit, push, install)
invertonator-diag.sh         one-shot Linux auto-hole diagnostics (run on Fedora, paste output)
Cargo.toml                   0.3.0-alpha.28 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix (5 platforms) + roadmap
docs/LINUX-TESTING.md        Fedora-first runbook (Wayland ext / X11 / KDE) +
                             holes test protocol + journal-line diagnosis
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION (alpha.28)
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
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v28, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           Super+F7 pierce holes, hole
                                                           manager submenu, fullscreen
                                                           auto-hole fallback, AT-SPI2
                                                           media lock-on v2.3 (per-window
                                                           rect-shader holes, event
                                                           capture), scanout guard,
                                                           v2.3.3 Super+F8 stored area
                                                           pins (select-once, title-token
                                                           matching, reboot-proof)
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
