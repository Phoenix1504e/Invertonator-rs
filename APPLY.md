# Invertonator-rs — Full Source Package (v0.3.0-alpha.8, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "v0.3.0-alpha.8: hole freshness (MagSetWindowSource IS the refresh) + per-kind media selection"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

---

## What alpha.8 fixes (your alpha.7 report)

> "the video in the hole is very laggy and other media are not
> recognised in the youtube app"

Two separate root causes, both fixed:

### 1. The hole-content slideshow ("video in the hole is very laggy")

alpha.7 stopped refreshing hole content except on moves/shows/failures
plus a 3s watchdog, based on a wrong assumption: that the Magnifier
control live-tracks the screen after one `MagSetWindowSource` call.
Your report disproved it — a playing video's rect never moves, so its
hole only advanced once per watchdog tick (~0.33Hz = a slideshow).
The OS Magnifier sample itself re-calls `MagSetWindowSource` on every
timer tick; that call IS the API's refresh mechanism, not churn.
(The real alpha.4–6 lag driver was the tracker's UIA COM storm, which
alpha.7 genuinely fixed — adaptive cadence + one-pass bulk cache.)

**Fix — corrected freshness policy in the hole pump:**
- video holes: re-source EVERY pump tick (33Hz — the OS sample's poll
  pattern; one small region, cheap),
- static holes: every ~600ms (1.7Hz — keeps thumbnails fresh when
  images load late or hovers change them),
- plus re-source on move / show, failure edge-logging unchanged,
- topmost re-assert rides the static cadence.

### 2. "Other media are not recognised" on YouTube

alpha.6 had a hard VIDEO PRIORITY: when ANY `<video>` element existed
on the page, ALL image/link candidates were discarded. On a YouTube
watch page the player is a `<video>` → the player kept its hole while
every sidebar video-thumbnail got nothing. Exactly what you saw.

**Fix — per-kind selection replaces hard priority:**
- per-kind dominance: within each kind (video / image / link),
  candidates keep a hole when they are ≥ 20% of the largest candidate
  OF THAT KIND. Watch page = player AND its sidebar thumbnails; a
  uniform thumbnail grid keeps its peers.
- unified 80×80px gate for ALL kinds including video: 1×1 ad-tracking
  `<video>` elements and 40–64px avatar/icon squares can never anchor
  a hole (this is what keeps the "small squares" bug dead).
- videos sort first before the hole cap — a capped page keeps its
  player before its thumbnails.
- hole cap raised 4 → 8 (pool is 16; static holes are cheap).

Also: the `tracker:` summary line now includes `v=/i=/l=` — the video/
image/link candidate counts that entered selection. A zero there means
the walk never SAW that kind; a nonzero with `holes=0` means selection
dropped it. One log round now pinpoints any remaining recognition gap.

---

## File map (14 files, latest state)

| # | File | Status | What it is |
|---|------|--------|-----------|
| 1 | `src/main.rs` | unchanged | Thin per-OS dispatcher |
| 2 | `src/win_main.rs` | unchanged | Windows backend — Magnifier API, UIA probe, tray, hotkeys, DPI |
| 3 | `src/common.rs` | **alpha.8** | Settings, logging — version 0.3.0-alpha.8 |
| 4 | `src/linux.rs` | unchanged | Linux X11 backend: XRandR gamma + `--self-test` |
| 5 | `src/holes.rs` | **alpha.8** | Freshness policy: video holes 33Hz, static 1.7Hz, topmost re-assert |
| 6 | `src/tracker.rs` | **alpha.8** | Per-kind dominance, unified 80px gate, videos-first cap, v/i/l log |
| 7 | `src/scrollhook.rs` | unchanged | Wheel-recency signal (alpha.7) |
| 8 | `Cargo.toml` | **EDITED** | v0.3.0-alpha.8 |
| 9 | `Cargo.lock` | **EDITED** | regenerated to match |
| 10 | `.github/workflows/build.yml` | unchanged | windows-latest + ubuntu-latest matrix |
| 11 | `README.md` | **EDITED** | alpha.8 status note |
| 12 | `docs/LINUX-TESTING.md` | unchanged | Linux tester runbook |
| 13 | `APPLY.md` | **EDITED** | this file |
| 14 | `.gitignore` | unchanged | `/target`, `*.exe` — keep it |

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

## Test checklist (Windows, alpha.8)

1. Quit any running instance: `Ctrl+Alt+F2`.
2. Optional clean log: delete `%LOCALAPPDATA%\Invertonator\log.txt`.
3. Start the new exe → screen inverts.
4. YouTube WATCH page (Chrome or LibreWolf) → the player hole's video
   should now advance smoothly (this is the main fix — alpha.7 updated
   it once per ~3s).
5. Same watch page → the sidebar video thumbnails should NOW get holes
   too (per-kind dominance). Expect roughly: 1 video hole + a few
   thumbnail holes, up to 8 total.
6. YouTube HOME grid → up to 8 thumbnail holes, still no tiny squares
   (80px gate) and no giant Frankenstein rects.
7. Let it sit ~1 minute, then check `tracker:` lines: the `v= i= l=`
   counts should be nonzero on YouTube (e.g. `v=1 i=7 l=0` on a watch
   page), and `iv=` should climb to 1600ms while idle.
8. Confirm nothing regressed: holes still topmost after clicking into
   the browser; scrolling still hides holes during the flick.
9. Send the whole `log.txt` either way — the `v=/i=/l=` breakdown plus
   the `holes:` lines answer "saw vs kept" in one round.
