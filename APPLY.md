# Invertonator-rs — Full Source Package (v0.3.0-alpha.12, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.12: 64Hz unlayered pump + offscreen/starved-rect filters (smoothness + precision)"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

**IMPORTANT — verify you are running the new build:** the FIRST line of
`%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.12 ===`. If it says alpha.11, the old
exe ran — re-download the CI artifact and start that one.

---

## What alpha.12 fixes (your alpha.11 report)

> "video is a little slow in the hole and radom holes still appear"

Two separate root causes, both code-proven.

### 1. "Video is a little slow in the hole" — the pump was slower than advertised

- The 30ms pump timer ran WITHOUT timer-resolution help: WM_TIMER fires
  on the ~15.6ms scheduler grid, so the "33Hz" poke actually landed
  every 31–46ms — an effective, wobbling 22–32Hz — while the page
  itself ran at 60fps. **Fix:** pump at 15ms + `timeBeginPeriod(1)`
  (undone on quit) → hole content now refreshes at ~64Hz, matching the
  display. This is the same rate Microsoft's own windowed-magnifier
  sample uses.
- Hole host windows were opaque WS_EX_LAYERED: every magnifier repaint
  was routed through the layered-window redirection surface — pure
  composition overhead. **Fix:** hosts are no longer layered (the
  proven probe config).
- New telemetry: once per 10s the log gets
  `holes: pump 63.9Hz ticks=639 holes=2` — the ACTUAL delivery rate on
  your machine. If video in the hole ever looks slow again, this line
  says whether the pump is the bottleneck.

### 2. "Random holes still appear" — two garbage-rect generators

- **Offscreen elements**: elements with `IsOffscreen=true`
  (display:none, scrolled out of view, virtualized list placeholders)
  carry stale or huge rectangles. A huge rect clamped into the window
  pins a hole to a random place. **Fix:** offscreen elements are now
  filtered on BOTH walk paths before they can become candidates.
- **Half-built trees**: right after a tab switch or page load, the
  browser hands out a PARTIAL accessibility tree; the walk that sees
  less than half the expected element count used to publish whatever
  placeholder rectangles it had. **Fix:** a starved walk publishes
  NOTHING — holes hide for a tick or two until a trustworthy walk
  completes (a full live walk or the finished materialization).
- The warm-up that measures the tree size now re-runs when the window
  title changes (tab switch / navigation), so a smaller next page is
  no longer mistaken for a half-built tree.

alpha.11 kept: evidence TTL (no completed walk in 8s → holes hide —
permanent holes remain structurally impossible) and tab-switch wake.

---

## Test protocol (2 minutes)

1. Start the new exe, confirm log line 1 = `v0.3.0-alpha.12` and the
   line `timer: timeBeginPeriod(1) -> NO_ERROR` right under it.
2. Play a YouTube video, F5. The video in the hole should now be as
   smooth as the page around it (~64Hz vs the old wobbling ~25Hz).
3. Switch tabs (Ctrl+Tab / click): holes must follow within ~0.1s and
   NO random rectangles may appear during or after the switch.
4. Scroll the page: no random rectangles from below-the-fold content.
5. After ~10s of inverted playback, log.txt contains a
   `holes: pump NN.NHz` line — if you ever report smoothness again,
   that number is the whole story.
6. If ANYTHING is still off: **drag the Desktop
   `invertonator-diagnostics.txt` straight into the chat** (auto-written
   10s after launch and 3s after every F5). It contains the dpi line,
   window rect, named candidate list, and the exact holes published —
   one paste replaces a whole round of guessing.

## File map (18 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts per push
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.12 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix + hotkeys
docs/LINUX-TESTING.md        X11 runbook (--self-test + live checklist)
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION
src/win_main.rs              Windows backend (tray, hotkeys, probe, DPI,
                             auto-diagnostics, timer resolution)
src/holes.rs                 hole pool + main-thread pump (15ms ≈64Hz,
                             unlayered hosts, evidence TTL, pump heartbeat)
src/tracker.rs               UIA media discovery (cached+live walks,
                             same-name merge, wake loop, offscreen +
                             starved-rect filters)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma-ramp inversion + --self-test
```

## After testing

Reply with what you saw (one line is enough — e.g. "video smooth,
random holes gone" or "still X"), and if anything is off, drag the
Desktop `invertonator-diagnostics.txt` into the chat. That file plus
the log's `holes: pump` / `SLOW walk` / `spec evidence stale` lines
pinpoint any remaining issue in ONE round.
