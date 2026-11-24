# Invertonator-rs

**System-wide dark mode that websites cannot see — Rust edition for Windows, Linux (X11) & GNOME/Wayland.**

![Invertonator](icon.svg)

> **Status: v0.3.0 stable — Linux v2.4.0 desktop-empty auto-off (GNOME/Wayland)**:
> **first stable cut** after 16 alpha field rounds. New in v2.4.0: while
> inversion is on, **closing the last window turns the inversion off
> automatically** — with no window left open the full-screen pass is just
> an inverted wallpaper, and that is exactly what you do not want on the
> home page. Same teardown as a manual Super+I (stored area pins survive
> in gsettings), dock/desktop-icon/notification-only screens count as
> empty, minimized windows still count as open, opt-out via
> `gsettings set org.gnome.shell.extensions.invertonator
> auto-off-no-windows false`. Ships on top of the alpha.29
> press-driven-button click fix.
> **Super+F7** pierces a true-color hole into the window under the
> pointer while everything around it stays inverted — the Windows
> F7-pierce analog (plain F7 is browser-claimed — caret browsing — so
> the default is browser-safe Super+F7), built as a second inversion
> pass on the picked window actor (double inversion = true color;
> drag/resize/fullscreen follow automatically).
> alpha.19 baseline: GNOME extension inversion that finally
> matches GNOME 51's shader API. Field-proven chain: alpha.16's ESM
> default-import crash → fixed in 17 (canonical `import * as Main`);
> alpha.17 revealed `set_shader_source()` is gone in GNOME 51; alpha.18
> revealed the `shader-source` property is gone too; the official
> "Port to GNOME 51" guide confirms the new contract — GLSL comes from a
> `Cogl.Snippet` via `vfunc_get_static_snippet()`. alpha.19 adopts the
> whole-desktop pattern proven by an extension shipping GLSL effects on
> GNOME 48 + 51: dual-API effect class (`Shell.GLSLEffect` ≤49 /
> `Clutter.ShaderEffect`+snippet 50+), attached to
> `Main.layoutManager.uiGroup` (windows + wallpaper + panel), texture via
> auto-provided `cogl_sampler0`. alpha.20 builds v2 on that machinery:
> the hole is the same effect attached twice (involution), toggled by
> Super+F7 (`toggle-pierce`, rebindable via gsettings — alpha.21 default
> after field tests showed plain F7 falls through to browsers), with
> panel-menu pierce/clear items and hole-count in the indicator; holes
> clear when inversion goes off; AT-SPI2 auto-media discovery is v2.1.
> alpha.21 hardening: keybinding registration journals itself (accel +
> action id at enable) and install.sh verifies every hotkey key is in
> the installed compiled schema.
> alpha.22 holes-everywhere (v2.1): a panel-menu HOLE MANAGER (one row
> per hole — title + class, click to close that hole); FULLSCREEN
> AUTO-HOLE — while inversion is on, a window going fullscreen
> (YouTube fullscreen, mpv, VLC, games) is pierced automatically
> true-color, the UIA auto-media analog heuristic edition
> (`auto-hole-fullscreen` gsettings opt-out; manual holes are never
> auto-removed by un-fullscreening); hole SURVIVAL — a hole reattaches
> when Mutter recreates the window actor and drops cleanly on window
> close; a DIRECT-SCANOUT guard while inverted (fullscreen video can
> never bypass the compositor shaders); override-redirect windows
> (menus, tooltips) are pierceable. Per-tab truth: a browser window is
> ONE buffer to the compositor — a hole covers whichever tab is
> showing and follows tab switches for free; element-level AT-SPI2
> rects are v2.2.
> alpha.23 MEDIA LOCK-ON (v2.2, the UIA tracker analog): while inversion
> is on, the focused window's AT-SPI2 tree is walked (budgeted, cached,
> chunked — D-Bus can never jank the shell) and every image/video/
> animation element ≥48px gets its OWN true-color rect hole; everything
> else stays inverted. Rect holes = a clipped Clutter.Clone of the
> window actor (identity overlay, scale-invariant math) carrying the
> invert effect — the involution trick, now at element granularity.
> Measured atspi→logical calibration (k = frame-width ratio) makes
> HiDPI/scale quirks self-correcting; rects follow scrolling (bounds-
> changed events → cheap extents refresh) and tab switches (document
> events → cached re-walk); focus change = context gone → holes clear
> (alpha.11 evidence-rule analog); hung apps blacklisted 60s; pool cap
> 6; `auto-hole-media` gsettings opt-out; toolkit-accessibility hint
> journaled when app trees may be dormant.
> alpha.26 RE-ROOT THRASH FIX (v2.3.2 — field round 11): the journal
> proved alpha.25's carry-over was being defeated — YouTube emits
> children-changed events continuously while playing and the walker
> re-rooted on each one, restarting inside the browser chrome forever
> (media=0 across six 480-call walks). alpha.26: only page loads
> re-root; children-changed just accelerates the schedule; drained+same
> window+no load = cheap extents refresh with revalidation at most
> every 8s; the BFS dives into DOCUMENT_* subtrees first (content doc
> reached in the first ticks); converging lines print app name +
> cache hits; new invertonator-diag.sh runs the discovery independently
> on the host and reports LibreWolf/Firefox a11y-hardening prefs.
> alpha.25 WALKER CONVERGENCE (v2.3.1 — fixes "still the holes are not
> automatic"): alpha.24's walk restarted from the tree top every 3s; with
> a cache miss on big pages (GI wrapper identity unguaranteed) the same
> first ~480 nodes were explored forever and media deeper in the tree was
> never reached. alpha.25: the BFS queue carries over between ticks — 480
> fresh nodes per tick, the walk drains in a few ticks, then holes publish;
> tree-shape events force a re-root (stale queue can never keep walking the
> old page); cache keys probe for stable object paths (journaled as
> key=path|wrap); event capture registers via add_callback with an
> instance-listener fallback and journals which API ran; a walk that
> starved journals "a11y walk converging — queue=N media=M".
> alpha.24 MEDIA LOCK-ON REWORK (v2.3 — fixes "auto hole doesn't work"):
> alpha.23's rect holes were Clutter.Clone overlays whose translation
> anchored the cloned window content at stage (0,0) — aligned only for
> windows already at the origin (fullscreen on the primary monitor) and
> visibly shifted copies for every windowed app, plus the 240-call walk
> budget starved on real web trees (a YouTube page's a11y tree is
> thousands of nodes). alpha.24: rect holes are now ONE shader per
> window whose GLSL bakes the rects as literal normalized coordinates
> (no uniforms — Cogl's GI uniform surface is unreliable) and inverts
> ONLY inside them, normalized against the actor's paint-volume box —
> the exact bounds Cogl's offscreen texture spans — so shadows/CSD
> margins cancel by construction and the hole rides the window actor
> (move/resize follow for free). Discovery adds FREE event capture
> (AT-SPI events carry their source element; media that announces
> itself costs zero walk budget) with window matching by rect
> containment, so it works even when pid/name resolution fails; the
> walk budget doubles to 480 and a budget-starved walk never wipes
> holes. PRIORITY FIX: element rects now BEAT the fullscreen auto-hole
> (fullscreen browser: video true, chrome still inverted — the v2.2
> request), the whole-window hole remains the fallback for native
> players/games, and it re-arms only on complete walks (no flip-flop).
> toolkit-accessibility is switched ON automatically (GTK3 apps need a
> restart to pick it up); every swap, fallback and resolution failure
> journals itself (a11y walk summaries, rect bake anchor, no-app-match
> one-shots).
> alpha.18 kept: install.sh honesty lines + APPLY zip-freshness gate.
> alpha.16 kept: GNOME/Wayland backend shipped (compositor-level
> fragment-shader stage inversion, Super+I, panel indicator, reboot
> persistence) + X11 binary session awareness + CI extension artifact.
> Windows alpha.15 kept: ANTI-FLAP HYSTERESIS for the center-square
> flapping (show needs 2 trusted sightings, hide needs 2 absences,
> geometric matching; `hyst_saves` / `hyst_held` telemetry).
> alpha.14 kept: fail-open offscreen (IsOffscreen is a HINT — Chromium
> flags visible media offscreen on some pages; the YouTube hole is
> built from exactly the image-kind layers the old hard gate ate) and
> context-aware evidence (the pump hides on staleness only when the
> foreground context changed; a slow-but-alive provider can no longer
> blank an unchanged page, and unconfirmed holes die within 4s of a
> tab switch).
> alpha.13 kept: sticky trust (starved ticks re-publish the last
> trusted hole set) + expedited live walk (wake walks never queue
> behind the 5s idle cooldown); starved/sticky counters in the
> diagnostics report.
> alpha.12 kept: ~64Hz unlayered pump (timeBeginPeriod(1)), warm-up
> re-keyed on title change.
> alpha.11 kept: evidence rule (holes never outlive their context) and
> tab-switch title wake (~50ms re-sync).
> alpha.10 kept: wake-on-demand discovery (scroll burst end / wheel /
> fg move-resize → walk in ~50ms), 800/1500ms playing cadence, startup
> auto-diagnostics.
> alpha.9 kept: SAME-NAME hole merge (unnamed fragments AND fragments
> sharing one accessible name merge across seams up to ~6% of their
> size; differently-named media keeps the 8px floor so grid thumbnails
> can never chain into a mega-hole) and every-hole poke (classification
> can no longer throttle hole content refresh).
> Diagnostics deliver THEMSELVES: 10s after launch, 3s after inversion
> comes ON (or on F10) the full report goes to the clipboard AND a
> Desktop file (invertonator-diagnostics.txt) — paste it into the bug
> report.
> Linux (X11): global gamma-ramp inversion shipped — the
> XComposite true-color-hole overlay is the next milestone. Linux (GNOME
> Wayland): compositor-level inversion shipped as a Shell extension —
> see [docs/LINUX-TESTING.md](docs/LINUX-TESTING.md). For the
> **fully-featured C# edition**, see
> [Invertonator](https://github.com/Phoenix1504e/Invertonator).

## Platform support

| Platform | Inversion | True-color media holes | Hotkeys |
|---|---|---|---|
| Windows 10+ | Magnifier API (compositor) | ✅ UIA tracker | `Ctrl+Alt+F5` invert · `F6` mode · `F7` pierce · `F2` quit |
| Linux (X11) | XRandR CRTC gamma | 🚧 v2 (XComposite overlay) — media inverts too for now | `Ctrl+Alt+I` toggle · `Ctrl+Alt+Q` quit |
| Linux (GNOME/Wayland) | Shell extension — compositor shader | ✅ **v2.4.0 shipped (v0.3.0 stable)** — Super+F7 pierce + hole manager + fullscreen video auto-hole + AT-SPI2 media lock-on (element rects beat the fullscreen hole) + Super+F8 stored area pins (drag → adjust handles → clickable Confirm/Redraw/Cancel, title-token matched, reboot-proof) + desktop-empty auto-off (last window closing turns inversion off) | `Super+I` invert · `Super+F7` pierce · `Super+F8` pin area · panel menu |
| Linux (KDE/Wayland) | KWin's built-in Invert effect | ❌ | user-bound |
| macOS | ❌ unsupported | ❌ | — |

If a Linux X11 run is killed while inverted, the ramp persists until you
run `invertonator --restore` (pushes identity ramps and exits). The
Wayland extension restores colors automatically on disable/quit. GNOME
holes are session-transient: toggling inversion off clears them —
re-pierce with Super+F7. Fullscreen windows auto-hole while inversion
is on (video: YouTube fullscreen, mpv, VLC); opt out with
`gsettings set org.gnome.shell.extensions.invertonator auto-hole-fullscreen false`.
Media lock-on (element-level image/video rects via AT-SPI2, default
on): opt out with `... auto-hole-media false`. Element rects beat the
fullscreen hole when found (fullscreen browser: video true, chrome
inverted); the extension enables
`org.gnome.desktop.interface toolkit-accessibility` itself — already-
running GTK3 apps need a restart to expose their trees (GTK4, Firefox
and Chromium usually react to the a11y bus alone).

No Linux box of your own? Run `invertonator --self-test` on any Linux
machine — it prints a paste-back diagnostic report (session type, gamma
hardware, live invert/restore round-trip, hotkey availability). Full
tester runbook — Wayland extension install, X11 binary, KDE note:
[docs/LINUX-TESTING.md](docs/LINUX-TESTING.md).

## The problem

Dark Reader and every dark-mode extension work by **modifying the page** —
injected CSS, changed DOM, `prefers-color-scheme` signals. All of it is
observable by the website. A site can detect — and react to — your dark mode.

## The idea

Invertonator never touches the web layer.

It applies a color-inversion transform at the **OS display compositor** —
*after* the browser has rendered, *after* every API a page could query.
Your browser presents the byte-identical fingerprint of a plain light-mode
session. No injected CSS. No DOM changes. No signals. There is nothing
in the web layer to detect.

Then comes the trick: for every image on screen, Invertonator carves a
**hole** — an overlay region running the *same inverse transform*. Media
passes through inversion twice and comes out in its **original colors**,
while everything around it stays dark.

> The macOS equivalent is Smart Invert, which relies on Apple-controlled
> framework integration unavailable on Windows. Invertonator-rs
> reconstructs that capability from userland, using public APIs: the
> Windows Magnification API + UI Automation — in Rust, with zero-cost COM
> and no garbage collector.

## Why Rust

This is a port of the [C# Invertonator](https://github.com/Phoenix1504e/Invertonator)
— same design, same calibrated constants, same product. The rewrite exists
because the remaining bottlenecks are language-shaped:

- **UIA cache requests** — one COM round-trip fetches the entire
  accessibility tree with properties. The C# binding layer made this
  impractical; the `windows` crate makes it natural.
- **No GC** — no generation-2 collection pauses during video.
- **No COM marshaling tax** — direct `windows`-crate calls.
- **Compile-time API safety** — the C# build once shipped with a
  hallucinated Win32 function name that failed *silently at runtime* for a
  week. In Rust, the linker refused to compile it in seconds. The mistakes
  get caught before the product does.
- **~0.5 MB binary, instant startup** — vs ~150 MB self-contained .NET.

## Current status (v0.2.0-alpha.2)

| Component | Status |
|---|---|
| Fullscreen Invert / Dim / Off | ✅ working |
| Tray icon + global hotkeys | ✅ working |
| settings.json (C#-compatible) | ✅ working |
| UIA cache-request tree walks | ✅ **proven** (1,152 nodes / 164 media on live YouTube) |
| Live-walk fallback engine | ✅ working |
| `MagSetWindowSource` / `SetTransform` / `SetColorEffect` | ✅ **verified from Rust** (F11 probe) |
| Media classification gates | 🚧 in tracker module |
| True-color media holes | 🚧 next release — **all APIs verified, build is transcription** |
| Browser-chrome preservation | 🚧 next release |
| Scroll-actuated adaptive cadence | 🚧 next release |

## Setup

**1.** Build (requires [Rust](https://rustup.rs) + MSVC Build Tools):

```
git clone https://github.com/Phoenix1504e/Invertonator-rs.git
cd Invertonator-rs
cargo build --release
```

**2.** Run `target\release\invertonator.exe` — the screen inverts.

**3.** Launch your browser with renderer accessibility enabled (lets
Invertonator *see* media elements — it does not modify the browser):

```
chrome.exe  --force-renderer-accessibility
msedge.exe  --force-renderer-accessibility
opera.exe   --force-renderer-accessibility
```

## Configuration

`settings.json` is **auto-created with defaults** next to the exe on first
run (same format as the C# edition — delete to reset):

```json
{
  "hotkeys": {
    "invert": "Ctrl+Alt+F5",
    "mode": "Ctrl+Alt+F6",
    "pierce": "Ctrl+Alt+F7",
    "quit": "Ctrl+Alt+F8",
    "dump": "Ctrl+Alt+F9"
  },
  "features": {
    "at_mode": false,
    "dim_level": 0.4
  }
}
```

- **Hotkeys** — `Ctrl`/`Shift`/`Alt`/`Win` + `F1–F12` / `A–Z` / `0–9` / named keys
- **`at_mode`** — announce as assistive technology system-wide (debugging aid)
- **`dim_level`** — Dim brightness multiplier (`0.05–1.0`, default `0.4`)

## Hotkeys (defaults)

| Keys | Action |
|---|---|
| `Ctrl+Alt+F5` | Invert on/off |
| `Ctrl+Alt+F6` | Cycle screen mode (Invert → Dim → Off) |
| `Ctrl+Alt+F2` | Quit (restores colors) |
| `Ctrl+Alt+F10` | UIA probe (diagnostics → `%LOCALAPPDATA%\Invertonator\log.txt`) |
| `Ctrl+Alt+F11` | Magnifier window probe (diagnostic — proves the hole APIs) |

## The architecture (why it can't be detected)

```
Website renders → Browser compositor → OS display compositor → Screen
                        ↑                      ↑
              pages can query          Invertonator acts HERE
              (nothing changed)        (nothing to query)
```

Every conventional dark mode acts *inside* the browser, where pages can
observe it. Invertonator acts one layer above — the transform is applied
to the final scan-out image, and no web API can see it.

Even privacy-hardened browsers (LibreWolf with `resistFingerprinting`,
Tor Browser) cannot detect it — there is nothing in the web layer to detect.

## Roadmap

- **v0.2.0-beta** — adaptive tracker (scroll-actuated discovery) + true-color
  media holes + browser-chrome preservation
- **v0.3** — event-driven UIA (instant discovery), auto-Dim on video,
  Firefox support
- **v2.0** — Linux true-color holes (X11 XComposite overlay · GNOME/Wayland
  per-window-actor inversion with a picked un-inverted window — **GNOME
  side shipped as alpha.20 Super+F7 pierce**), AT-SPI2 discovery
- **v2.1** — **GNOME holes-everywhere shipped as alpha.22**: hole manager
  menu, fullscreen video auto-hole, actor-recreate survival, scanout
  guard, OR-window pierce
- **v2.2** — **GNOME element-level media lock-on shipped as alpha.23**:
  AT-SPI2 image/video rects (clone rect-holes), scroll/tab follow,
  measured calibration, hung-app blacklist
- **v2.3** — **media lock-on rework shipped as alpha.24** (fixes "auto
  hole doesn't work"): rect-shader holes replacing the misaligned clone
  overlay, event-capture discovery, element rects beat the fullscreen
  auto-hole (whole-window hole = native player/game fallback),
  toolkit-accessibility auto-enable, self-journaling swaps/fallbacks
- **v2.3.1** — **walker convergence shipped as alpha.25** (fixes "still
  the holes are not automatic"): BFS queue carry-over between ticks (a
  starved walk makes real progress instead of restarting), stable cache
  keying with journaled mode, re-root on tree-shape events, event-API
  fallback, bake-clip diagnostics
- **v2.3.2** — **re-root thrash fix shipped as alpha.26** (field round
  11, journal-diagnosed): children-changed events no longer re-root the
  walk (YouTube emits them continuously while playing), steady-state
  extents-refresh instead of per-tick re-roots, document-subtree BFS
  priority, invertonator-diag.sh host-side probe incl. browser
  a11y-hardening detection; still queued: per-element pooling beyond 6,
  tracker telemetry
- **v2.3.3** — **stored area pins shipped as alpha.27** (your "select an
  area once and it gets stored for YouTube"): Super+F8 drag-select,
  normalized to the focused window frame, title-token matched,
  gsettings-persisted (reboot-proof), re-applied the moment a matching
  window shows up
- **v2.3.4** — **adjust + confirm shipped as alpha.28**: the selection
  preview gets 8 resize handles, drag-to-move, drag-outside redraw and a
  Confirm/Redraw/Cancel bar — nothing is stored until Confirm
- **v2.3.5** — **dead-on-click fix shipped as alpha.29** (field round
  16, risk Q3): press-driven buttons + grab-actor hit-test fallback so
  Confirm/Redraw/Cancel fire under every Mutter grab-routing behavior
- **v2.4.0 — v0.3.0 STABLE** — **desktop-empty auto-off** (your "no
  windows open → inversion turns off"): the last real window closing
  turns the inversion off with a notification, exactly like a manual
  Super+I; docks/desktop-icons/notifications count as empty, minimized
  windows still count as open, event-driven (no polling — Super+I on an
  already-empty desktop stays on), opt-out gsettings key
  `auto-off-no-windows`
- **research** — compositor-level video handling, other Wayland compositors

## Credits & thanks

- The [windows crate](https://github.com/microsoft/windows-rs) — raw Win32
  and COM with zero marshaling tax
- The [Magnification API](https://learn.microsoft.com/en-us/windows/win32/api/_magapi/) —
  the unsung hero
- [NegativeScreen](https://github.com/mlaily/NegativeScreen) — the pioneer
- [tracing](https://github.com/tokio-rs/tracing) — the diagnostics that
  caught every bug, including one hallucinated API name
- Apple — for Smart Invert, the target reconstructed from userland

## License

[MIT](LICENSE)
