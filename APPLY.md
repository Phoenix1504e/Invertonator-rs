# Invertonator-rs — Full Source Package (v0.3.0-alpha.11, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.11: evidence TTL (stale holes self-hide) + tab-switch title wake + slow-walk warnings"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

**IMPORTANT — verify you are running the new build:** the FIRST line of
`%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.11 ===`. If it says alpha.10, the old
exe ran — re-download the CI artifact and start that one.

---

## What alpha.11 fixes (your alpha.10 report)

> "holes are permanent even after shifting tabs"

Root cause, code-proven: a hole lives exactly as long as the tracker's
last published spec set, and the tracker re-publishes ONLY when a walk
completes. A tab switch (Ctrl+Tab, tab click) fires **no wheel event**,
keeps the **same window** and the **same rect** — none of the alpha.10
wake signals fire — and worse, the walk issued right after the switch
talks to a browser that is tearing down and rebuilding its
accessibility provider. Such a cross-process walk can block for a very
long time. While it blocks, the last hole set sits on screen FOREVER
(the pump re-draws it at 33Hz from stale specs). If the walk even
panics the tracker thread, same freeze.

Three changes make that failure mode structurally impossible:

### 1. Evidence TTL — no hole outlives its evidence (holes.rs)

Every completed walk re-stamps an evidence clock (even a walk that
found nothing). The pump — which runs on the main thread, fully
independent of the tracker thread — now **hides every hole** whose
spec set has not been re-confirmed within **8 seconds**. Whatever
wedged the tracker (hung provider, crashed thread), the worst case is
"holes hide after 8s and come back when the walk unblocks" — never
"permanent holes". The log gets one `holes: spec evidence stale —
hiding all holes` warning when this fires, and a
`fresh spec evidence — holes re-enabled` line on recovery.

### 2. Tab-switch wake — the tracker now SEES tab switches (tracker.rs)

The 50ms wake loop now also compares the foreground window's TITLE.
Switching tabs changes the title (almost always) and costs the tracker
one cheap caption read per slice — no UIA traffic. Title changed →
walk within ~50ms → new tab's holes (or no holes) replace the old set
in under a tenth of a second, scroll-fast.

### 3. Slow-walk warnings — the wedge leaves a fingerprint (tracker.rs)

Any completed walk slower than 1s logs
`tracker: SLOW walk NNNNms — provider busy or wedged (tab teardown?)`.
If holes ever feel frozen/laggy again, log.txt now says whether the
browser's accessibility provider was the bottleneck — no guessing.

---

## Test protocol (2 minutes)

1. Start the new exe, confirm log line 1 = `v0.3.0-alpha.11`.
2. Open two YouTube tabs (one playing). Press **F5** → holes appear.
3. **Ctrl+Tab / click to the other tab** → the old holes must move or
   clear almost immediately (~0.1s). Switch back and forth a few times,
   also try switching to a page with NO video (holes should clear).
4. Leave the video tab alone for 30s — holes must stay correct and the
   video smooth (alpha.9/10 behavior unchanged).
5. If ANYTHING is still off: the diagnostics file already exists on
   your Desktop (`invertonator-diagnostics.txt`, written 10s after
   launch and 3s after every F5) — **drag that file straight into the
   chat**, no copying needed. It contains the dpi line, window rect,
   named candidate list, and the exact holes the tracker published.

## File map (18 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts per push
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.11 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix + hotkeys
docs/LINUX-TESTING.md        X11 runbook (--self-test + live checklist)
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION
src/win_main.rs              Windows backend (tray, hotkeys, probe, DPI,
                             auto-diagnostics)
src/holes.rs                 hole pool + main-thread pump (30ms, ~33Hz
                             poke, evidence TTL)
src/tracker.rs               UIA media discovery (cached+live walks,
                             same-name merge, wake loop, tab-switch wake)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma-ramp inversion + --self-test
```

## After testing

Reply with what you saw (one line is enough — e.g. "holes now follow
tab switches, video smooth" or "still X on tab Y"), and if anything is
off, drag the Desktop `invertonator-diagnostics.txt` into the chat.
That file plus log.txt's `SLOW walk` / `spec evidence stale` lines
pinpoint any remaining issue in ONE round.
