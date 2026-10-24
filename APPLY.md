# Invertonator-rs — Full Source Package (v0.3.0-alpha.14, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.14: fail-open offscreen + context-aware evidence (no hole in youtube, round 2)"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

**IMPORTANT — verify you are running the new build:** the FIRST line of
`%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.14 ===`. If it says alpha.13, the old
exe ran — re-download the CI artifact and start that one.

---

## What alpha.14 fixes (your alpha.13 report)

> "no hole in youtube"

alpha.13 removed the flash-then-vanish loop, but YouTube stayed empty.
Two independent causes remained, and both are fixed now:

1. **The offscreen gate was eating YouTube's hole.** alpha.12 added a
   filter: elements Chromium flags `IsOffscreen` never become holes.
   Chromium ALSO flags visible media offscreen on some pages — and the
   YouTube hole is built from exactly the IMAGE-kind poster/player
   layers your own dumps showed (`img*3[YouTu…]`). They were suppressed
   on BOTH walk paths, silently, so every walk returned zero candidates
   and the holes were gone everywhere, forever. VSCodium's elements are
   never flagged, which is why it stayed "perfection".
   **Now:** the flag is only a hint. An offscreen-flagged element is
   still offered as a hole when its geometry is independently usable —
   inside the window, and either video-kind or at least a quarter of
   the window's smaller dimension (≥270px on 1080p). The clamp, size
   gates and giant filter still re-judge every rect, so the old
   "random holes" garbage stays out.

2. **The 8s evidence TTL could hide still-correct holes.** The pump hid
   every hole that hadn't been re-confirmed by a completed walk within
   8s — even when nothing on screen had changed (a slow YouTube
   provider under load can take seconds per walk).
   **Now:** the pump hides stale holes only when the FOREGROUND
   CONTEXT changed since the last publish (window + page title). Same
   window, same title = the video the holes describe is provably still
   on screen, so slow walks can't blank it. Any context change (tab
   switch, alt-tab, retitle) hides unconfirmed holes within 4s — the
   tab-switch guarantee got FASTER, not weaker.

alpha.13 kept: sticky trust + expedited live walk. alpha.12 kept: ~64Hz
unlayered pump. Also new: `offscreen_admits` in the diagnostics report
and a one-line log canary the first time an offscreen-flagged element
is admitted — so if holes ever appear where none should, the log says
exactly that.

---

## Test protocol (2 minutes)

1. Start the new exe, confirm log line 1 = `v0.3.0-alpha.14`.
2. Open YouTube, play a video, F5. The player hole must appear and
   STAY — no flash-then-vanish, no minutes-long absence.
3. Scroll the page: holes follow immediately after the scroll ends.
4. Switch tabs (Ctrl+Tab / click) away and back: the old holes clear
   within ~4s worst case (usually ~1s), the new page's holes appear
   promptly, and NO random rectangles at any point.
5. Alt-tab VSCodium ↔ browser: VSCodium hole stays as perfect as you
   reported; YouTube hole stays alive the whole time.
6. If ANYTHING is still off: **drag the Desktop
   `invertonator-diagnostics.txt` straight into the chat** (auto-written
   10s after launch and 3s after every F5). It now includes
   `tracker: starved_ticks= sticky_held= offscreen_admits=` — one paste
   tells me whether YouTube's walks starve, what the pump holds, and
   whether the fail-open path fired. No guessing needed.

## File map (18 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts per push
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.14 (windows+linux deps)
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
                             starvation + offscreen telemetry in report)
src/holes.rs                 hole pool + main-thread pump (15ms ≈64Hz,
                             unlayered hosts, context-aware evidence,
                             pump heartbeat)
src/tracker.rs               UIA media discovery (cached+live walks,
                             same-name merge, wake loop, fail-open
                             offscreen, sticky trust + expedited live)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma-ramp inversion + --self-test
```

## After testing

Reply with what you saw (one line is enough — e.g. "youtube holes stay,
vscodium still perfect" or "still X"), and if anything is off, drag the
Desktop `invertonator-diagnostics.txt` into the chat. That file plus
the log's `holes: pump` / `starved=` / `offscreen-flagged` /
`SLOW walk` lines pinpoint any remaining issue in ONE round.
