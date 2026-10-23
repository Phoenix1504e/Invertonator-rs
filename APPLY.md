# Invertonator-rs — Full Source Package (v0.3.0-alpha.7, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "v0.3.0-alpha.7: video smoothness — event-driven re-source + adaptive UIA walk"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Note: this package adds a `.gitignore` (`/target`, `*.exe`) — keep
it. Without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

---

## What alpha.7 fixes ("the video is very laggy")

Three stacked performance bugs, all fixed:

1. **The hole pump re-called `MagSetWindowSource` 33× per second on
   video holes.** The Magnifier control updates its content LIVE once
   the source is set — that is how the OS magnifier works. alpha.4–6
   re-set the same source every 30ms "to keep it fresh"; every call
   tears down and rebuilds the capture pipeline, so a hole parked on a
   video forced 33 capture resets per second.
   **Fix — event-driven re-source policy:**
   - re-source when the hole rect actually moved (with a 1px jitter
     epsilon so UIA round-off can't churn it),
   - once when a hidden hole becomes visible,
   - ~3.3Hz retry while a call FAILS (stale content must not persist),
   - a 3s watchdog as freeze insurance — which also re-asserts
     TOPMOST (the old 600ms topmost churn is gone too).

2. **The UIA tracker hammered the browser ~25×/s.** Discovery ran a
   full walk every 400ms no matter what; on LibreWolf that's a ~4,000
   node live walk — and every property read is a cross-process COM
   call the browser's UI thread must service, competing with video
   decode. **Fix — adaptive cadence:** while the foreground window AND
   its hole set are stable, the interval doubles 400 → 800 → 1600ms
   (4× less traffic). Any change — app switch, navigation (title
   change), moved hole rects, wheel scrolling — snaps it back to fast
   immediately; the sleep wakes early on those events, so alt-tab and
   scroll responsiveness do not suffer.

3. **The Gecko live fallback read properties per node.** 3–4
   `Current*` COM round trips per node, every walk.
   **Fix:** the live walk now goes through `FindAllBuildCache` — all
   properties are filled in ONE cross-process pass; per-node `Current*`
   reads remain only as a fallback.

Also: the `tracker:` summary line now ends with `iv=…ms walk=…ms`
(current discovery interval + last walk duration) so the log shows the
backoff working and how expensive each walk is on your machine.

---

## File map (14 files, latest state)

| # | File | Status | What it is |
|---|------|--------|-----------|
| 1 | `src/main.rs` | unchanged | Thin per-OS dispatcher |
| 2 | `src/win_main.rs` | unchanged | Windows backend — Magnifier API, UIA probe, tray, hotkeys, DPI |
| 3 | `src/common.rs` | **alpha.7** | Settings, logging — version 0.3.0-alpha.7 |
| 4 | `src/linux.rs` | unchanged | Linux X11 backend: XRandR gamma + `--self-test` |
| 5 | `src/holes.rs` | **alpha.7** | Event-driven re-source policy, watchdog, move epsilon |
| 6 | `src/tracker.rs` | **alpha.7** | Adaptive cadence + nap, FindAllBuildCache live walk, iv=/walk= log |
| 7 | `src/scrollhook.rs` | **alpha.7** | + `ms_since_wheel()` wheel-recency signal |
| 8 | `Cargo.toml` | **EDITED** | v0.3.0-alpha.7 |
| 9 | `Cargo.lock` | **EDITED** | regenerated to match |
| 10 | `.github/workflows/build.yml` | unchanged | windows-latest + ubuntu-latest matrix |
| 11 | `README.md` | **EDITED** | alpha.7 status note |
| 12 | `docs/LINUX-TESTING.md` | unchanged | Linux tester runbook |
| 13 | `APPLY.md` | **EDITED** | this file |
| 14 | `.gitignore` | **NEW** | `/target`, `*.exe` — keep it |

---

## Linux (X11) — unchanged from alpha.3

- XRandR CRTC gamma ramps: `[0..255] → [255..0]` at scanout — below the
  web layer, so sites still can't see it.
- Starts INVERTED. `Ctrl+Alt+I` toggles, `Ctrl+Alt+Q` quits (both
  restore your original ramps; Drop does it even on panic).
- `invertonator --restore` — recovery after a SIGKILL while inverted.
- `invertonator --self-test` — one-command diagnostic; exit 0 =
  machine supported.
- Media is inverted too for now (true-color holes = v2, XComposite).
- Wayland sessions are detected and refused with a clear message.

---

## Validation status of this package

- `cargo check` + `cargo clippy`: ZERO warnings on both
  `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`.
- Linux release build + `--self-test` runs clean.
- CI builds both artifacts after your push.

---

## Test checklist (Windows, alpha.7)

1. Quit any running instance: `Ctrl+Alt+F2`.
2. Optional clean log: delete `%LOCALAPPDATA%\Invertonator\log.txt`.
3. Start the new exe → screen inverts.
4. Chrome or LibreWolf → a YouTube WATCH page → the player hole should
   now show SMOOTH video (this is the main fix — compare with alpha.6).
5. Give it a minute, then check the log's `tracker:` lines: `iv=` should
   climb to 1600ms while you just watch (that's the backoff saving your
   browser ~25 walk passes/second), and drop back toward 400ms the
   moment you scroll, switch apps, or the page changes.
6. Scroll a page with a video → holes hide during the flick and come
   back on the player (wheel events now also wake the tracker early).
7. Resize the window / toggle fullscreen → hole follows the player
   (rect change → immediate fast re-walk).
8. Confirm nothing regressed: holes still topmost after clicking into
   the browser; home grid ≤ 4 thumbnail holes; no scattered squares.
9. Send the whole `log.txt` either way — `iv=`/`walk=` plus the
   `holes:` lines tell us exactly how the new policy behaved.
