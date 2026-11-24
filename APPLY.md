# Invertonator-rs — Full Source Package (v0.3.0 stable — desktop-empty auto-off)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

**This is the STABLE release line (0.3.0, Linux v2.4.0) — first stable cut
after 16 alpha field rounds. It ships the new desktop-empty auto-off
feature on top of the alpha.29 click fix. Test protocol below was
extended accordingly (step 10).**

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

## Round 15 field event — CI: `cargo check --locked` exit 101

GitHub Actions failed with:

```
error: cannot update the lock file …/Cargo.lock because --locked was
passed to prevent this
help: to generate the lock file without accessing the network, remove
the --locked flag and use --offline instead.
```

Root cause: **Cargo.lock was stale, not the code.** `Cargo.toml` was
bumped alpha.24 → alpha.28 across five commits, but the lock file was
last touched at alpha.23 (`version = "0.3.0-alpha.23"`). CI runs
`cargo check --locked`, which REFUSES to touch the lock file when the
manifest moved — so the job died before compiling anything. No
dependency changed since alpha.23 (verified: `git diff ddd5aa2 HEAD --
Cargo.toml` touches only the version line), so the complete fix is a
one-line version sync inside Cargo.lock → `0.3.0-alpha.28` (commit
alpha.28a, already inside this zip). Do NOT hand-edit the lock beyond
that, and do NOT remove `--locked` from CI — the flag is what keeps
your builds reproducible; keep the lock in sync instead.

---

## What's new in v0.3.0 stable (Linux v2.4.0) — desktop-empty auto-off

Your request: *"if there are no windows open and we are on the home page
then inversion turns off — add this as a feature, it is just an ease of
life thing."* Shipped exactly that:

- While inversion is on, **the moment the last real window closes** the
  inversion turns itself off — with a notification — exactly like a
  manual Super+I: holes die, the media walker stops, stored area pins
  keep their gsettings entries, `start-enabled` is cleared, the panel
  icon flips back. The inverted-wallpaper-on-an-empty-desktop problem is
  gone.
- **Counted as real windows**: NORMAL, DIALOG, MODAL_DIALOG, UTILITY,
  SPLASHSCREEN. A dash-to-dock (DOCK), a desktop-icons layer (DESKTOP),
  notifications and menus never hold the inversion open.
- **A minimized window still counts as open** (it is still open —
  restoring it must not need a re-toggle). Windows on OTHER workspaces
  also count: the inversion goes off when the last window CLOSES, not
  when you visit an empty workspace.
- **Event-driven, no polling**: the trigger is the per-window
  `unmanaged` signal — the one hook every close path (window closed,
  app quit, crash, force-kill) passes through. Because there is no
  poll, pressing Super+I on an ALREADY-empty desktop stays on
  (pre-arming before you launch the next app works), and the login
  auto-resume (`start-enabled`) behaves the same.
- **Opt out** without losing the feature forever:
  `gsettings set org.gnome.shell.extensions.invertonator auto-off-no-windows false`
- A Super+F8 selection left open while the last window closes is
  cancelled cleanly (an orphaned grab overlay would have frozen input).

## Round 16 field event — Confirm / Redraw / Cancel dead on click (fixed)

Your report: "the buttons which show after an area is selected — confirm,
redraw and cancel — do not work on clicking." This was pre-registered as
risk Q3 ("St.Button 'clicked' vs modal grab routing"). Root cause: on some
Mutter builds the modal grab routes button PRESSes to the grab actor
instead of the picked child — a St.Button never forms its press→release
pair and `clicked` never emits, while drags (handled by the grab actor
itself) keep working. alpha.29 makes the click path double-layered so it
cannot depend on that routing anymore:

1. **Press-driven buttons** — Confirm/Redraw/Cancel now fire on PRESS (the
   event that is guaranteed to deliver), not on `clicked`.
2. **Overlay hit-test fallback** — the grab actor's own press handler
   (field-proven delivery) additionally hit-tests the press coordinates
   against the buttons AND the 8 resize handles (`_pinButtonAt` /
   `_pinToolbarAt` / `_pinHandleAt`). Whichever layer receives the press
   first fires the action exactly once — the other consumes with
   EVENT_STOP, so there is no double-fire in either Mutter behavior.

Keyboard shortcuts (Enter / R / Esc) were never affected and remain.

---

## What alpha.29 changes (v2.3.5)

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

It must print **`Invertonator-rs <sha> 0.3.0`**. If it
prints alpha.29 or older, re-download the zip from the chat first.

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
verifies the zip is v0.3.0 (metadata version 30 + adjust/confirm +
auto-off markers) BEFORE touching the repo, refuses a dirty repo, resets
to `origin/main`, extracts, commits `v0.3.0 stable: desktop-empty
auto-off — last real window closing turns inversion off (v2.4.0;
includes alpha.29 click fix)`, pushes, runs the installer and prints the
logout reminder. Zip somewhere else? `bash apply-linux.sh /path/to/zip`.

Manual fallback (only if you want to do it by hand — NO guards):

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "v0.3.0 stable: desktop-empty auto-off — last real window closing turns inversion off (v2.4.0; includes alpha.29 click fix)" && \
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

After login: install.sh output should include SEVEN
`schema check: '...' present` lines (toggle-invert, toggle-pierce,
pin-hole, auto-hole-media, auto-hole-fullscreen, auto-off-no-windows,
start-enabled).

## Test protocol (6 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 30**, State: ACTIVE.
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
8. **the alpha.28/29 test (adjust + confirm, then the YouTube pin):**
   inverted, on a YouTube watch page (windowed), press **Super+F8** →
   screen dims with a hint label. Drag a box around the video player and
   RELEASE — a solid preview appears with 8 handles, a `W × H px` readout
   and a Confirm / Redraw / Cancel bar (journal:
   `area preview — drag the handles…`). Drag an edge handle to fine-tune,
   drag inside the rect to move it — journal stays silent (nothing
   stored yet). Press **Enter** (or click Confirm). Journal:
   `area stored — match title tokens [song x, youtube] rel=[…]`.
   **CLICK test (the alpha.29 fix):** Super+F8 again, drag, release, then
   click **Redraw** (dashed box returns, journal: `redraw requested`),
   drag again, release, click **Cancel** (overlay closes, journal:
   `area selection cancelled`), drag once more, release, click
   **Confirm area** — journal: `area stored`. Every button click must
   act; if one is still dead, send the journal tail.
   Press Super+F8 again, drag, then **R** or Redraw → fresh selection;
   Esc mid-adjust → nothing stored. Close the tab, open ANOTHER video on
   YouTube → within ≤2s a true-color rect appears at the SAME relative
   position (journal: `stored area applied (title […])`) — no walker
   needed. Resize the window → the rect follows. Fullscreen the video →
   the pin drops (`stored area dropped (window went fullscreen)`) and
   normal fullscreen behavior returns; leave fullscreen → pin comes
   back. Panel menu → "Stored areas" lists the pin; click the row to
   forget it.
9. **NEW — do stored holes survive a restart? (the alpha.28a answer:
   YES — audited, see below).** After step 8 stored a pin, prove it is
   on disk first:
   ```bash
   gsettings get org.gnome.shell.extensions.invertonator pinned-holes
   # → [{"cls":"…","tokens":["song x","youtube"],"rel":[0.03,…]}, …]
   ```
   That key lives in dconf (`~/.config/dconf/user`) — it is written the
   INSTANT you press Enter/Confirm, not at logout. Now reboot (or log
   out + back in; X11: `Alt+F2`, `r`). After login inversion resumes
   automatically (`start-enabled`); open a YouTube watch page → the
   true-color rect returns at the SAME relative position within ≤2s,
   journal: `stored area applied (title […])`. No re-selection. Also
   try panel-menu inversion OFF → ON: pin effects are destroyed with
   the shaders but the entries stay in gsettings and re-apply on ON.
   Fullscreen windows still drop pins by design (windowed layout is
   meaningless there) — un-fullscreen and the pin returns.
10. **NEW — the v0.3.0 stable test: desktop-empty auto-off (your request,
    "no windows open → inversion turns off").** With inversion ON and two
    windows open, close ONE of them → inversion MUST stay on (journal:
    nothing new). Close the LAST window → the desktop wallpaper comes
    back to normal colors IMMEDIATELY, a notification appears ("No
    windows open — inversion turned off automatically…") and the journal
    prints `last real window closed — desktop empty, inversion auto-off`
    followed by `inversion OFF`. The panel indicator flips to
    `Inversion: OFF`. Sub-cases that must NOT fire (or must fire) the
    auto-off:
    a) minimize the last window instead of closing it (still open — stays
       inverted),
    b) switch to an empty workspace while windows exist on another one
       (still open — stays inverted),
    c) only dash-to-dock / desktop icons / notifications on screen after
       the last app closes → those are not windows → auto-off DOES fire,
    d) Super+I pressed on an ALREADY-empty desktop (inversion stays ON —
       pre-arming; it auto-offs only when the next app's window is opened
       and closed again),
    e) opt-out check: `gsettings set org.gnome.shell.extensions.invertonator
       auto-off-no-windows false`, invert, close the last window → stays
       ON (set the key back to `true` afterwards).
    Also confirm pins survived the auto-off: `gsettings get
    org.gnome.shell.extensions.invertonator pinned-holes` still lists the
    step-8 entry, and a fresh Super+I re-applies it to a matching window.
11. If AT-SPI discovery ALSO still works on your setup, both can coexist:
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
if ($env:OS -ne 'Windows_NT') { throw 'WINDOWS PowerShell block — wrong machine.' } else { cd C:\Users\admin\invertonator-push; Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force; git add -A; git commit -m "v0.3.0 stable: desktop-empty auto-off — last real window closing turns inversion off (v2.4.0; includes alpha.29 click fix)"; git push origin HEAD }
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (26 tracked files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
apply-linux.sh               ONE-command guarded Fedora apply (verify zip, commit, push, install)
invertonator-diag.sh         one-shot Linux auto-hole diagnostics (run on Fedora, paste output)
Cargo.toml                   0.3.0 stable (windows+linux deps)
Cargo.lock                   synced to 0.3.0 (same commit — CI `--locked` stays green)
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix (5 platforms) + roadmap
docs/LINUX-TESTING.md        Fedora-first runbook (Wayland ext / X11 / KDE) +
                             holes test protocol + journal-line diagnosis
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION (v0.3.0)
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
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v30 (0.3.0 stable), shells 45–51
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
                                                           matching, reboot-proof),
                                                           v2.4 desktop-empty auto-off
                                                           (last window closing turns
                                                           inversion off; gsettings
                                                           auto-off-no-windows)
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
