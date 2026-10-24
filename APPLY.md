# Invertonator-rs — Full Source Package (v0.3.0-alpha.15, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.15: anti-flap hysteresis (random square comes and goes)"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

**IMPORTANT — verify you are running the new build:** the FIRST line of
`%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.15 ===`. If it says alpha.14, the old
exe ran — re-download the CI artifact and start that one.

---

## What alpha.15 fixes (your latest report)

> "a random square comes and goes in the center of the screen"

Root cause found in the code: the publish path had NO smoothing. Every
trusted walk result went to the pump 1:1 — a rect seen in one walk
appeared instantly, and a rect the next walk missed was dropped
instantly. On YouTube the trusted walks are the heavy LIVE walks every
5s (the cached walk is chronically half-tree there — alpha.13's
diagnosis), so any live-to-live difference in what Chromium reports
toggled a hole on/off with a ~5s period. One square, center screen,
comes and goes — that is the exact signature. VSCodium's candidate set
is identical every walk, which is why it stayed perfect.

**Now:** the publish path runs an anti-flap hysteresis (two buckets):

1. **Show needs 2 sightings** — a rect brand-new to the screen publishes
   only after two consecutive trusted walks saw it. A one-walk wonder
   (Chromium reporting an element exactly once) never shows at all.
   Two fast exceptions, on purpose: a fresh page (previous walk saw
   nothing — the player still appears on the FIRST live walk after
   navigation) and WAKE walks (scroll end, tab switch, alt-tab, F5 —
   user-initiated freshness stays instant, the alpha.9/10/11 fast
   paths are untouched).

2. **Hide needs 2 absences** — a rect one walk stopped seeing stays on
   screen for exactly one missed trusted walk. One flaky live walk
   that misses the player no longer blanks it for 5s. The second
   consecutive miss drops it, so nothing lingers forever.

Matching is geometric ("same hole" = intersection ≥ 50% of the smaller
rect): identical rects match trivially, a scrolled/moved hole matches
its old rect and takes the NEW geometry immediately (position tracking
never waits), and distinct grid items across a gutter share almost
nothing so they can't mask each other.

Guarantees kept: tab switches clear old holes the same tick (wake
flush); scroll ends re-sync positions instantly (wake flush); F5-ON
shows holes in ~50ms (wake flush); the pump's 4s context TTL is still
the last-resort backstop. VSCodium: every walk matches the previous —
the hysteresis is a no-op there.

New telemetry: the diagnostics report now ends with
`tracker: starved_ticks= sticky_held= offscreen_admits= hyst_saves=
hyst_held=`. A climbing `hyst_saves` while a square comes and goes =
live-walk flakiness caught and smoothed (proof the fix engaged);
`hyst_held` counts suppressed one-walk wonders. The log also records
the first save and the first hold with rects.

---

## Test protocol (2 minutes)

1. Start the new exe, confirm log line 1 = `v0.3.0-alpha.15`.
2. Open YouTube, play a video, F5. The player hole must appear as fast
   as alpha.14 (first live walk, ~1-2s) and STAY — no blink-out when
   nothing is happening, no random square anywhere.
3. Scroll the page: holes follow immediately after the scroll ends.
4. Switch tabs (Ctrl+Tab / click) away and back: the old holes clear
   within ~4s worst case (usually ~1s), the new page's holes appear
   promptly, NO stale rectangles at any point.
5. Alt-tab VSCodium ↔ browser: VSCodium stays as perfect as you
   reported; YouTube hole stays alive the whole time.
6. Watch the center square specifically: it should either never appear
   again or (if it was a real element) appear and STAY stable instead
   of cycling.

## If ANYTHING is still off — one paste, full diagnosis

**Drag the Desktop `invertonator-diagnostics.txt` straight into the
chat** (auto-written 10s after launch and 3s after every F5; F10
regenerates it any time on the affected window). It now includes
`hyst_saves= hyst_held=` plus everything from alpha.14 — one paste
tells me whether the flapping is live-walk misses (hyst_saves
climbing), suppressed phantoms (hyst_held climbing), or something else
entirely. Six rounds of symptom sentences have carried the bug this
far; the file ends it in one round.

## File map (18 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts per push
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.15 (windows+linux deps)
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
                             starvation + offscreen + hysteresis telemetry)
src/holes.rs                 hole pool + main-thread pump (15ms ≈64Hz,
                             unlayered hosts, context-aware evidence,
                             pump heartbeat)
src/tracker.rs               UIA media discovery (cached+live walks,
                             same-name merge, wake loop, fail-open
                             offscreen, sticky trust + expedited live,
                             ANTI-FLAP HYSTERESIS)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma-ramp inversion + --self-test
```

## After testing

Reply with what you saw (one line is enough — e.g. "square gone,
youtube hole stays, vscodium still perfect" or "still X"), and if
anything is off, drag the Desktop `invertonator-diagnostics.txt` into
the chat. That file plus the log's `hysteresis kept/held` /
`starved=` / `SLOW walk` lines pinpoint any remaining issue in ONE
round.
