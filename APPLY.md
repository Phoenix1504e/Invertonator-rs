# Invertonator-rs — Full File Rewrite Package (v0.3.0-alpha.6, cross-platform)

Complete final contents of every file that changed vs. your GitHub
`origin/main`. No patches — replace whole files. The layout mirrors the
repo exactly, so you can copy the folder over your clone.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "v0.3.0-alpha.6: hole candidate selection — video priority, dominance filter, flatness probe"
git push origin HEAD
```

The zip is the ENTIRE repo at the fixed state — extract over your clone,
commit, push. No `.git` inside, your history is untouched. After the
push, the Actions tab builds BOTH artifacts (Windows exe + Linux binary).

---

## What alpha.6 fixes ("same random rectangle + small squares on UI elements")

alpha.5 scoped WHERE candidates may live (inside the page area). alpha.6
fixes WHICH candidates earn a hole at all — and adds the diagnostics
that were silently missing:

1. **Every ≥40px in-page image became a hole.** YouTube pages carry
   dozens of 40–120px avatars, badges and icons; with `MAX_HOLES = 16`
   they all qualified → the "small squares scattered on ui elements".
   **Fix — three selection gates, in order:**
   - **VIDEO PRIORITY** — if the walk saw any real `<video>` element,
     ONLY videos become holes. A watch page gets exactly the player.
   - **DOMINANCE** — without videos, a candidate must reach 20% of the
     largest survivor's area. Avatars/icons next to a real preview sit
     at 1–6% and die. A uniform media grid keeps up to 4 peers.
   - **FLATNESS PROBE** — an image candidate whose on-screen pixels are
     uniform (5×5 GetPixel grid: low luminance spread + ≤4 coarse
     colors) is a spacer or a bogus rect parked on flat page area — no
     hole. Works on the inverted desktop too (inversion preserves
     variance). Videos are exempt (a paused talking-head can be flat).
   - Image size floor raised 40 → 64 px; hole cap 16 → 4.

2. **The any-touch union merge built Frankenstein holes.** Any two
   candidates sharing even one pixel row/column were unioned — rects
   that merely shared a corner grew into one huge misplaced hole (the
   "random rectangle" look).
   **Fix:** merge only when the intersection is ≥ 40% of the smaller
   candidate (real img-inside-link containment still merges).

3. **Document scoping now also clamps.** A candidate's containing
   Document rect becomes the clamp boundary — a hole can no longer poke
   outside its page, only inside window ∩ document ∩ virtual screen.

4. **DPI status logging was dead code.** `set_dpi_awareness()` ran
   BEFORE the logger existed, so its lines never reached log.txt — on
   your machine we never actually saw whether per-monitor-v2 engaged.
   **Fix:** the status is returned and logged after logging init, plus
   a numeric readout (`dpi: system=… window=…` — 96 = 100% scale).

5. **Mag/Move failures were silent.** A failed `MagSetWindowSource`
   after a move leaves the PREVIOUS region's content displayed at the
   new position — literally "a rectangle showing the wrong part".
   **Fix:** results are checked; failures log on edge (one line per
   failure onset/recovery, never per tick).

---

## alpha.5 recap (already in this package)

- Bogus UIA rects poking >8px outside their window are dropped.
- Live-walk candidates are document-pane scoped (media only inside the
  page area); `tracker:` summary logs `docs=`.
- alpha.4: topmost hole overlays that survive app focus; throttled
  discovery summary; alpha.3: Gecko live-walk fallback.

---

## File map (12 files, latest state)

| # | File | Status | What it is |
|---|------|--------|-----------|
| 1 | `src/main.rs` | unchanged | Thin per-OS dispatcher |
| 2 | `src/win_main.rs` | **alpha.6** | Windows backend — Magnifier API, UIA probe, tray, hotkeys, DPI status logging |
| 3 | `src/common.rs` | **alpha.6** | Settings, logging — version 0.3.0-alpha.6 |
| 4 | `src/linux.rs` | unchanged | Linux X11 backend: XRandR gamma + `--self-test` |
| 5 | `src/holes.rs` | **alpha.6** | Hole pool: kind tags in logs, Mag/Move failure edge-logging |
| 6 | `src/tracker.rs` | **alpha.6 — selection** | Video priority, dominance, flatness probe, tightened merge, doc clamp |
| 7 | `src/scrollhook.rs` | unchanged | WH_MOUSE_LL wheel hook |
| 8 | `Cargo.toml` | **EDITED** | v0.3.0-alpha.6 |
| 9 | `Cargo.lock` | **EDITED** | regenerated to match |
| 10 | `.github/workflows/build.yml` | unchanged | windows-latest + ubuntu-latest matrix |
| 11 | `README.md` | **EDITED** | alpha.6 status note |
| 12 | `docs/LINUX-TESTING.md` | unchanged | Linux tester runbook |

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

## Test checklist (Windows, alpha.6)

1. Quit any running instance: `Ctrl+Alt+F2`.
2. Optional clean log: delete `%LOCALAPPDATA%\Invertonator\log.txt`.
3. Start the new exe → screen inverts. **Check the first log lines
   contain `dpi: per-monitor-v2 aware` and `dpi: system=96 window=96`
   (or 120/144 etc. if your display is scaled — that's fine, as long
   as the awareness line says per-monitor-v2).**
4. Chrome → a YouTube WATCH page → expect ONE hole, on the player, in
   true colors — not 16 scattered squares. Sidebar thumbnails and
   avatars must stay inverted.
5. YouTube HOME page → up to 4 holes on the biggest thumbnails only.
6. Scroll / resize / click around — holes follow and stay visible.
7. LibreWolf → same watch-page test (live-walk path).
8. If anything still lands wrong: F10 probe on that app + send the
   WHOLE `log.txt`. The `tracker:` lines now show each kept hole WITH
   its kind letter (`V`=video, `I`=image, `L`=link) and exact rect, and
   `holes: #n shown` lines show what the pump actually placed — compare
   those numbers against where the hole sits on screen and the
   mismatch pinpoints the remaining bug in one round.

**Linux (X11 session)** — full details in `docs/LINUX-TESTING.md`.
