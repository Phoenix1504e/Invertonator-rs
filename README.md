# Invertonator-rs

**System-wide dark mode that websites cannot see — Rust edition for Windows, Linux (X11) & GNOME/Wayland.**

![Invertonator](icon.svg)

> **Status: v0.3.0-alpha.17** — GNOME extension hotfix. alpha.16's extension
> failed to LOAD on every GNOME version — journal fingerprint:
> `SyntaxError: The requested module 'resource:///org/gnome/shell/ui/main.js'
> doesn't provide an export named: 'default'`. The shell's ui modules
> (main.js, panelMenu.js, popupMenu.js) export NAMED bindings only and have
> NO default export; the canonical ESM template imports them as
> `import * as Main`. All three imports fixed (extension now reaches
> enable(): State ACTIVE, panel icon, Super+I), and the ShaderEffect
> constructor property moved to the canonical camelCase `shaderType`.
> install.sh no longer silently skips enabling on a pre-login install: when
> `gnome-extensions enable` cannot see the fresh extension it whitelists
> the uuid in org.gnome.shell enabled-extensions via gsettings, so the next
> login comes up ACTIVE with zero manual steps (alpha.16 field finding:
> the pre-login enable was swallowed and the extension loaded DISABLED
> forever).
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
| Linux (GNOME/Wayland) | Shell extension — compositor shader | 🚧 v2 (per-window-actor inversion + picked hole) | `Super+I` toggle · panel menu |
| Linux (KDE/Wayland) | KWin's built-in Invert effect | ❌ | user-bound |
| macOS | ❌ unsupported | ❌ | — |

If a Linux X11 run is killed while inverted, the ramp persists until you
run `invertonator --restore` (pushes identity ramps and exits). The
Wayland extension restores colors automatically on disable/quit.

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
  per-window-actor inversion with a picked un-inverted window), AT-SPI2
  discovery
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
