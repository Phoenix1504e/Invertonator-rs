# Invertonator-rs — Full File Rewrite Package (v0.3.0-alpha.4, cross-platform)

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
git commit -m "v0.3.0-alpha.4: topmost hole overlays + on-screen clamp + tracker discovery summary"
git push origin HEAD
```

The zip is the ENTIRE repo at the fixed state — extract over your clone,
commit, push. No `.git` inside, your history is untouched. After the
push, the Actions tab builds BOTH artifacts (Windows exe + Linux binary).

---

## What alpha.4 fixes (from your alpha.3 field log)

Your F10 probe log cracked the case. The probes were healthy — all three
apps exposed huge trees and the live walk classified media everywhere.
Three real defects hid behind that:

1. **Holes were getting buried in the Z-order (the big one).** The hole
   host windows were created WITHOUT `WS_EX_TOPMOST`, and they were only
   ever shown with `ShowWindow` on the hidden→shown transition. So the
   moment you clicked/focused the browser, the app raised ABOVE the hole
   windows and the pump never re-raised them. That is exactly the
   "VSCodium hole appeared, then disappeared" report — and why browser
   holes seemed to never appear (they flashed before your first click,
   then sat buried behind Chrome/LibreWolf forever).
   **Fix:** hosts are now `WS_EX_TOPMOST`, shown via
   `SetWindowPos(HWND_TOPMOST, SWP_SHOWWINDOW | SWP_NOACTIVATE)`, and
   topmost is re-asserted on the static refresh cadence (~600ms). No
   normal app window can ever cover a hole again. Click-through is
   unchanged.

2. **Hole rects were never clamped to the screen.** Your log showed the
   Chrome "player" image at `(-218,-109)` and a stray LibreWolf element
   at `y=-11526` — a Magnifier window sourcing off-screen pixels
   captures garbage or nothing, and wastes a pool slot.
   **Fix:** every candidate is clamped to `foreground window ∩ virtual
   screen`; candidates that end up below their own size gate or less
   than half visible are dropped.

3. **The tracker was a black box.** The log proved the probe's view but
   said nothing about what the tracker produced per 400ms tick.
   **Fix:** throttled discovery summary in log.txt — one line per state
   change (min 2s apart) plus a 10s heartbeat:
   `tracker: fg="..." nodes=4135 cands=311 holes=3 [(x,y) WxH dyn, ...]`
   Plus the hole pump now logs every show/hide with the rect:
   `holes: #0 shown @(0,0) 1186x931`.

Also learned from your log: on YOUR machine even Chromium and VSCodium
return empty CachedChildren (the probe logged the Gecko guard for all
three apps) — so the alpha.3 live-walk fallback is the operative path
everywhere. It held up: 419/4135/188 nodes walked, media found in every
app. alpha.4 keeps that architecture and makes its output land on screen.

---

## File map (12 files, latest state)

| # | File | Status | What it is |
|---|------|--------|-----------|
| 1 | `src/main.rs` | unchanged | Thin per-OS dispatcher |
| 2 | `src/win_main.rs` | **alpha.3** | Windows backend — Magnifier API, UIA probe, tray, hotkeys, DPI |
| 3 | `src/common.rs` | **alpha.4** | Settings, logging — version bumped to 0.3.0-alpha.4 |
| 4 | `src/linux.rs` | unchanged | Linux X11 backend: XRandR gamma + `--self-test` |
| 5 | `src/holes.rs` | **alpha.4 — Z-order fix** | Topmost hole pool + show/hide logging |
| 6 | `src/tracker.rs` | **alpha.4 — clamp + summary** | UIA media tracker: clip-to-window, throttled discovery log |
| 7 | `src/scrollhook.rs` | unchanged | WH_MOUSE_LL wheel hook |
| 8 | `Cargo.toml` | **EDITED** | v0.3.0-alpha.4 |
| 9 | `Cargo.lock` | **EDITED** | regenerated to match |
| 10 | `.github/workflows/build.yml` | unchanged | windows-latest + ubuntu-latest matrix |
| 11 | `README.md` | **EDITED** | alpha.4 status note |
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

## Browsers (Windows)

- The inversion itself is browser-independent — display-level.
- Holes: the alpha.3 live walk finds media on Chromium AND Gecko;
  alpha.4 makes the resulting holes visible (topmost) and correctly
  placed (clamped). On Gecko the fallback has no document-pane scoping,
  so some large browser-chrome images may also get holes — cosmetic,
  tunable later with F10 census data.

---

## Validation status of this package

- `cargo check` + `cargo clippy`: ZERO warnings on both
  `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`.
- CI builds both artifacts after your push.

---

## Test checklist (Windows, alpha.4)

1. Quit any running instance first: `Ctrl+Alt+F2`.
2. Optional clean log: delete
   `%LOCALAPPDATA%\Invertonator\log.txt`.
3. Start the new exe → screen inverts.
4. Chrome → YouTube video, wait 3s → hole over the player area.
5. **Click the browser, scroll a bit** — the hole must now STAY (this
   was the alpha.3 killer: focus stole the hole).
6. LibreWolf → same test.
7. VSCodium → hole persists while the element is visible.
8. `Ctrl+Alt+F7` → holes hide; again → holes return (topmost).
9. F10 probe on each app (optional — log richness).
10. Send me `log.txt` — it now contains `tracker:` summary lines and
    `holes: #n shown` lines that pinpoint anything still wrong.

**Linux (X11 session)** — full details in `docs/LINUX-TESTING.md`.
