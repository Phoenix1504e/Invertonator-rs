# Invertonator-rs — Full File Rewrite Package (v0.3.0-alpha.5, cross-platform)

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
git commit -m "v0.3.0-alpha.5: hole placement hardening — bogus-rect guard + document-pane scoping"
git push origin HEAD
```

The zip is the ENTIRE repo at the fixed state — extract over your clone,
commit, push. No `.git` inside, your history is untouched. After the
push, the Actions tab builds BOTH artifacts (Windows exe + Linux binary).

---

## What alpha.5 fixes ("holes placed very badly")

alpha.4 made the holes VISIBLE (topmost overlays — that part worked).
alpha.5 fixes WHERE they land. Three placement defects, all visible in
your earlier field logs:

1. **Bogus UIA rects became holes.** The Chrome "player" image at
   `(-218,-109)` pokes OUTSIDE its own window — impossible for real
   on-page media. After alpha.4's clamping it became a giant hole over
   the wrong part of the window; LibreWolf's `y=-11526` element was the
   same disease.
   **Fix:** any candidate rect poking more than 8px past a window edge
   is DROPPED, not clamped. Real media always lives inside its window.

2. **No document-pane scoping on the live walk.** The cached walk only
   collected media inside `Document` subtrees; the flat live fallback
   classified EVERYTHING — toolbar icons, page furniture, browser
   chrome — so holes sprinkled across random UI.
   **Fix:** the live walk now harvests every Document element's rect
   (free — no extra UIA calls) and requires each candidate's center to
   sit inside a document rect. Browser chrome and out-of-page elements
   get no holes. Apps with no documents keep the old behavior.

3. **The giant bogus rect dominated the merge.** Sorted largest-first,
   it swallowed every overlapping candidate into one huge misplaced
   hole. Fixed transitively by (1).

The tracker summary log now also reports `docs=` (documents seen) so the
next log tells us whether scoping was active per app.

---

## alpha.4 recap (already in this package)

- Hole hosts are `WS_EX_TOPMOST`, shown via `SetWindowPos(HWND_TOPMOST,
  SWP_SHOWWINDOW | SWP_NOACTIVATE)`, topmost re-asserted ~every 600ms.
  No normal app window can cover a hole; holes survive app focus.
- Pump logs every hole show/hide with rect; tracker logs a throttled
  discovery summary (`tracker: fg=… nodes= cands= docs= holes= [rects]`).
- alpha.3's live-walk fallback stays: on your machine even Chromium and
  VSCodium return empty CachedChildren, so the live walk is the
  operative path everywhere — and it held up (419/4135/188 nodes,
  media found in every app).

---

## File map (12 files, latest state)

| # | File | Status | What it is |
|---|------|--------|-----------|
| 1 | `src/main.rs` | unchanged | Thin per-OS dispatcher |
| 2 | `src/win_main.rs` | **alpha.3** | Windows backend — Magnifier API, UIA probe, tray, hotkeys, DPI |
| 3 | `src/common.rs` | **alpha.5** | Settings, logging — version 0.3.0-alpha.5 |
| 4 | `src/linux.rs` | unchanged | Linux X11 backend: XRandR gamma + `--self-test` |
| 5 | `src/holes.rs` | **alpha.4** | Topmost hole pool + show/hide logging |
| 6 | `src/tracker.rs` | **alpha.5 — placement** | Bogus-rect guard, document scoping, clamp, summary log |
| 7 | `src/scrollhook.rs` | unchanged | WH_MOUSE_LL wheel hook |
| 8 | `Cargo.toml` | **EDITED** | v0.3.0-alpha.5 |
| 9 | `Cargo.lock` | **EDITED** | regenerated to match |
| 10 | `.github/workflows/build.yml` | unchanged | windows-latest + ubuntu-latest matrix |
| 11 | `README.md` | **EDITED** | alpha.5 status note |
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
- CI builds both artifacts after your push.

---

## Test checklist (Windows, alpha.5)

1. Quit any running instance: `Ctrl+Alt+F2`.
2. Optional clean log: delete `%LOCALAPPDATA%\Invertonator\log.txt`.
3. Start the new exe → screen inverts.
4. Chrome → YouTube video → holes should sit ON media only: the player
   area / thumbnails — NOT the toolbar, tab strip, or half the window.
5. Click around, scroll — holes must stay put and stay visible.
6. LibreWolf → same test.
7. VSCodium → hole only on real content, persists.
8. If anything still lands wrong: F10 probe on that app + send
   `log.txt` — the `tracker:` lines now include `docs=` and exact hole
   rects, which pinpoint the remaining placement issue in one round.

**Linux (X11 session)** — full details in `docs/LINUX-TESTING.md`.
