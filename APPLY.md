# Invertonator-rs — Full Source Package (v0.3.0-alpha.9, cross-platform)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## Apply

Easiest path (clone already on disk at `C:\Users\admin\invertonator-push`):

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "v0.3.0-alpha.9: incremental UIA refresh + 66Hz unlayered hole pump + classifier breadth"
git push origin HEAD
```

After the push, the Actions tab builds BOTH artifacts (Windows exe + Linux
binary). Keep the `.gitignore` this package ships (`/target`, `*.exe`) —
without it, `git add -A` after a local build would sweep thousands of
`target/` build artifacts into your GitHub repo.

**IMPORTANT — verify you are running the new build:** the FIRST line of
`%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.9 ===`. If it says alpha.8, the old exe
ran — re-download the CI artifact and start that one.

---

## What alpha.9 fixes (your alpha.8 report)

> "still very laggy and still doesnt recognises the media outside
> the player"

Two separate root causes attacked, plus a telemetry line that makes the
next round one-log-diagnosable either way.

### 1. "Still very laggy" — the browser was drowning in a11y queries

alpha.7/8 fixed the COM-per-node storm, but the tracker still did a FULL
tree walk (one `FindAllBuildCache` over every node on the page) every
400ms–1.6s. On a YouTube-sized page (~4k nodes) that is a huge
serialization burst served by the **browser's UI thread** — the thread
that renders the video. Even at 1.6s intervals that's recurring jank
exactly where you see it: in the video.

**Fix — incremental rect-refresh steady state:**
- The heavy full walk now runs ONLY on real triggers: app switch,
  navigation (title change), window move/resize, a stale element, or
  every 5s.
- Between full walks, a light pass every 250ms re-reads ONLY the kept
  holes' rectangles (≤ 8 tiny cross-process reads) and re-runs the local
  selection math. Steady-state UIA traffic drops ~1000×.
- The `tracker:` summary line now carries `mode=full` / `mode=incr` and
  `refresh=Nms` so the log shows both passes.

**Plus three hole-rendering fixes:**
- The pump now runs at **15ms with timeBeginPeriod(1)** — without that
  call a 30ms timer drifts on the 15.6ms scheduler grid, so the old
  "33Hz" actually stuttered between 21–32Hz. Now video holes re-source
  at ~66Hz = per-frame on a 60Hz display.
- Hole hosts are **no longer WS_EX_LAYERED** — an opaque alpha-255
  layered window forces every magnifier repaint through DWM redirection
  composition. The proven F11 probe host was never layered; nothing
  needed layering. Pure overhead, gone.
- Topmost hygiene is now bit-gated (the WS_EX_TOPMOST style is checked
  and restored only when actually lost; the unconditional z-order
  re-assert dropped from every 600ms to every ~1.8s).

**New telemetry — the `holes: stat` line, every 10s:**
`holes: stat hz=66 src=66 avg=180us max=900us fail=0`
- `hz` = the pump's ACTUAL tick rate (timer starvation check)
- `src` / `avg` / `max` = how many `MagSetWindowSource` refreshes fired
  and what each cost on YOUR machine
- `fail` = failed refresh calls
If video is still not smooth, this one line says whether the re-source
itself is the bottleneck (high avg/max) or something else (low avg).

### 2. "Doesn't recognise media outside the player" — classifier was too narrow

alpha.8's per-kind selection fixed the DROPPING side, but the SEEING
side still only knew four localized strings ("video", "graphic",
"image", "link") plus two numeric types. Media exposed under any other
name never even became a candidate.

**Fix — wider net at the SEEING stage:**
- `thumbnail` / `movie` / `media` / `animation` / `picture` / `photo`
  localized control types now classify (video-ish → video kind, the
  rest → image kind).
- **ClassName fallback**: Chromium reports the HTML tag name as the UIA
  ClassName — so a `<video>` whose provider skips the localized string
  still classifies via class name "Video".
- The cache request now also fetches ClassName (one extra property in
  the same single cross-process pass — no extra traffic).
- The F10 probe census matches the same rules, so a probe in the
  YouTube app shows exactly what the tracker CAN see.
- alpha.9 also unifies document-pane scoping onto the cached walk path
  (it previously only applied to the live-fallback path) — browser
  chrome can no longer produce holes on either path.

Still not recognized after this? The log now pins down WHERE it dies:
`v=/i=/l=` = how many video/image/link candidates entered selection;
`holes=` = how many survived. A zero count = the walk never saw it (send
the F10 census — it prints the provider's LocalizedControlType
inventory); nonzero with `holes=` too low = selection dropped it.

---

## Test checklist (copy into your reply)

1. First log line says `v0.3.0-alpha.9`? (yes/no)
2. `dpi:` line says "per-monitor-v2 aware"? `dpi: system=?/window=?`
3. Play a YouTube video ~30s, then send the WHOLE log.txt. It should
   contain `holes: stat` lines (every ~10s) and `tracker:` lines with
   `mode=full` / `mode=incr`.
4. In the YouTube app (the page where media outside the player is
   missing), press **F10 once**, then **F2 to quit** (flushes the log),
   restart the app if you want, and send log.txt. The `uia-probe live
   census` lines list every control-type string the app exposes — that
   inventory decides the next classifier widening if needed.
5. Smoothness verdict: smoother / same / worse than alpha.8.

## File map (14 files, latest state)

```
Cargo.toml            0.3.0-alpha.9 (windows+linux deps, lto release)
Cargo.lock            pinned
.gitignore            /target, *.exe — keep, protects your clone
README.md             status + build matrix + hotkeys
APPLY.md              this file
docs/LINUX-TESTING.md X11 runbook (--self-test + live checklist)
src/main.rs           cross-platform entry
src/common.rs         settings.json + logging + VERSION
src/win_main.rs       Windows backend (tray, hotkeys, UIA probe,
                      mag probe, DPI, timeBeginPeriod)
src/holes.rs          hole pool + main-thread pump (15ms, telemetry)
src/tracker.rs        UIA media discovery (full walk + incremental
                      refresh, classifier, selection pipeline)
src/scrollhook.rs     low-level wheel hook (scroll-hold, wake signals)
src/linux.rs          X11 XRandR gamma-ramp inversion + --self-test
icon.svg              logo (README)
```

## After testing

Send back:
1. the full `%LOCALAPPDATA%\Invertonator\log.txt` (quit with **F2**
   first so everything is flushed to disk),
2. your test checklist answers,
3. what you saw (holes where? laggy? which app/page?).

The log's `mode=full|incr`, `holes: stat`, `v=/i=/l=`, and the probe
census together pinpoint any remaining issue in ONE round — no more
guessing.
