# Invertonator-rs

**System-wide dark mode that websites cannot see — Rust edition for Windows & Linux (X11).**

![Invertonator](icon.svg)

> **Status: v0.3.0-alpha.7** — Windows: UIA media tracker + Magnifier-API
> true-color holes proven on hardware (1,152-node cache-request walks ·
> 164 media elements classified · MagSetColorEffect/Source/Transform all
> verified). Gecko browsers (Firefox/LibreWolf) supported via automatic
> live-walk fallback when a provider leaves CachedChildren empty.
> alpha.4: topmost hole overlays + tracker discovery summary logging.
> alpha.5 hole-placement hardening: bogus UIA rects poking outside their
> window are dropped, live-walk candidates are document-pane scoped
> (media only inside the page area), survivors clamped to the visible
> window region. alpha.6 candidate selection: <video> elements take
> priority (a watch page gets exactly the player), dominance filter
> kills avatar/icon swarms, GetPixel flatness probe drops uniform
> rects, tightened overlap merge, DPI status + Mag failure logging.
> alpha.7 video smoothness: MagSetWindowSource is no longer re-called
> 33×/s on video holes (the Magnifier control updates live; re-source
> only on move/show/failure + 3s watchdog), the UIA walk backs off
> 400ms→1.6s while the hole set is stable, and the Gecko live fallback
> builds its property cache in ONE cross-process pass instead of
> ~30k per-second COM reads into the browser.
> Linux (X11): global gamma-ramp inversion shipped — the
> XComposite true-color-hole overlay is the next milestone. For the
> **fully-featured C# edition**, see
> [Invertonator](https://github.com/Phoenix1504e/Invertonator).

## Platform support

| Platform | Inversion | True-color media holes | Hotkeys |
|---|---|---|---|
| Windows 10+ | Magnifier API (compositor) | ✅ UIA tracker | `Ctrl+Alt+F5` invert · `F6` mode · `F7` pierce · `F2` quit |
| Linux (X11) | XRandR CRTC gamma | 🚧 v2 (XComposite overlay) — media inverts too for now | `Ctrl+Alt+I` toggle · `Ctrl+Alt+Q` quit |
| Linux (Wayland) | needs compositor backends (KWin effect / GNOME ext) | ❌ | — |
| macOS | ❌ unsupported | ❌ | — |

If a Linux run is killed while inverted, the ramp persists until you run
`invertonator --restore` (pushes identity ramps and exits).

No Linux box of your own? Run `invertonator --self-test` on any Linux
machine — it prints a paste-back diagnostic report (session type, gamma
hardware, live invert/restore round-trip, hotkey availability). Full
tester runbook: [docs/LINUX-TESTING.md](docs/LINUX-TESTING.md).

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
- **v2.0** — Linux/X11 backend (XRandR inversion, AT-SPI2 discovery)
- **research** — Wayland, compositor-level video handling

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
