# Invertonator-rs — Full Source Package (v0.3.0-alpha.13, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.13: sticky trust + expedited live walk (YouTube holes no longer vanish)"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

**IMPORTANT — verify you are running the new build:** the FIRST line of
`%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.13 ===`. If it says alpha.12, the old
exe ran — re-download the CI artifact and start that one.

---

## What alpha.13 fixes (your alpha.12 report)

> "vs codium hole works like perfection but the holes on youtube are gone now"

VSCodium being perfect was the key clue: its accessibility tree is small
and complete, so its walks are never "starved". YouTube is the opposite —
the cheap cached walk chronically returns a HALF-BUILT tree there
(Chromium's cached view never approaches the full-tree population that
the warm-up FindAll forces). alpha.12's new guard reacted to every such
walk by publishing NOTHING, and the only walk that actually FINDS the
video (the heavy live FindAll) runs on a 5s cooldown — so the holes
flashed for under a second and vanished for ~4 seconds, in a loop.
"gone now", exactly.

Two changes, and both are invisible on never-starved pages like
VSCodium:

1. **Sticky trust** — a starved tick no longer publishes empty. It
   re-publishes the last TRUSTED hole set (the latest live-walk or
   full-cached result), so the holes stay on screen between live walks
   instead of blinking out. A genuinely media-free page still clears
   the holes as before (healthy empty walks replace the sticky set).
2. **Expedited live walk** — a wake walk (scroll end, wheel, window
   move/resize, tab switch, alt-tab, F5-ON) that comes back starved
   now settles briefly and, if the tree is still half-built, runs the
   live FindAll IMMEDIATELY. The 5s cooldown still caps the idle
   live-walk rate (that is the anti-stutter budget); user-initiated
   freshness must not queue behind it. Tab switches and alt-tabs get a
   600ms settle first — the same teardown window the old code walked
   straight into.

Also new: an offscreen-video canary in the log (if Chromium ever flags
the visible player as IsOffscreen, the alpha.12 filter would eat the
video hole silently — now it leaves a `VIDEO element flagged
IsOffscreen` line), and starved/sticky counters in the tracker
heartbeat and in the diagnostics report
(`tracker: starved_ticks=N sticky_held=N`).

alpha.12 kept: ~64Hz unlayered pump, IsOffscreen filter, warm-up
re-keyed on title change. alpha.11 kept: evidence TTL (no completed
walk in 8s → holes hide — permanent holes remain structurally
impossible) and tab-switch wake.

---

## Test protocol (2 minutes)

1. Start the new exe, confirm log line 1 = `v0.3.0-alpha.13`.
2. Open YouTube, play a video, F5. The video hole must appear and STAY
   (no flash-then-vanish loop).
3. Switch tabs (Ctrl+Tab / click) away and back: holes must follow —
   the old tab's holes clear within ~1s, the new tab's appear within
   ~1s (the settle wait is the price for not walking a tearing-down
   tree), and NO random rectangles at any point.
4. Scroll the page: holes follow immediately after the scroll ends.
5. Alt-tab VSCodium ↔ browser: VSCodium hole stays as perfect as you
   reported; YouTube hole stays alive the whole time.
6. If ANYTHING is still off: **drag the Desktop
   `invertonator-diagnostics.txt` straight into the chat** (auto-written
   10s after launch and 3s after every F5). It now includes the
   `tracker: starved_ticks= sticky_held=` line — one paste tells me
   whether YouTube's walks are still starving and what the pump is
   holding, no guessing needed.

## File map (18 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts per push
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.13 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix + hotkeys
docs/LINUX-TESTING.md        X11 runbook (--self-test + live checklist)
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION
src/win_main.rs              Windows backend (tray, hotkeys, probe, DPI,
                             auto-diagnostics, timer resolution,
                             starvation telemetry in the report)
src/holes.rs                 hole pool + main-thread pump (15ms ≈64Hz,
                             unlayered hosts, evidence TTL, pump heartbeat)
src/tracker.rs               UIA media discovery (cached+live walks,
                             same-name merge, wake loop, offscreen filter,
                             sticky trust + expedited live walk)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma-ramp inversion + --self-test
```

## After testing

Reply with what you saw (one line is enough — e.g. "youtube holes stay,
vscodium still perfect" or "still X"), and if anything is off, drag the
Desktop `invertonator-diagnostics.txt` into the chat. That file plus
the log's `holes: pump` / `starved=` / `SLOW walk` lines pinpoint any
remaining issue in ONE round.
