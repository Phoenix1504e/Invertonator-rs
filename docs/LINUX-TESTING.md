# Testing Invertonator on Linux — a 10-minute runbook

The maintainer develops on Windows, so Linux confirmation has to come from
real machines. This runbook is written so **anyone with a Linux desktop**
can test the app in ten minutes and send back a useful report — no build
tools, no Rust, no special knowledge required.

---

## What you need

- Any Linux desktop running an **X11 session** (Ubuntu/Xorg, Linux Mint,
  XFCE, KDE on X11, i3, Cinnamon … all fine).
- Nothing else. The binary loads `libX11`/`libXrandr` at runtime — every
  desktop distro already ships them.
- On a **Wayland** session? See the Wayland section at the bottom first.

---

## Step 1 — get the binary (no build needed)

1. Open the repo on GitHub → the **Actions** tab → the latest **Build** run.
2. Download the artifact **`Invertonator-rs-linux-x64-latest`**.
3. Unzip it and make it executable:

```bash
unzip Invertonator-rs-linux-x64-latest.zip
chmod +x invertonator
```

(Alternative: build from source — `cargo build --release`. The result is
the same binary.)

## Step 2 — the one-command diagnostic

```bash
./invertonator --self-test
```

This prints a full report: session type, X server identity, every output
with its gamma hardware, a live invert/restore round-trip (this catches
drivers that silently ignore gamma changes), and whether the hotkeys are
free. It changes nothing visually — ramps are restored within the test.

**Send back the ENTIRE output.** Exit code `0` means the machine supports
gamma inversion; `1` means a real failure; `2` means the environment
wasn't available (e.g. no X display).

## Step 3 — the visual test (any browser — literally any)

Inversion happens at the display (gamma LUT) level, *below* every
application — so the browser brand genuinely does not matter here.

```bash
./invertonator
```

1. The screen should invert **immediately** on launch.
2. Open any browser — Firefox, Chromium, Chrome, LibreWolf, Falkon,
   anything — and browse normally. Text should be crisp (this is a
   hardware LUT, not a blurry magnifier hack).
3. Images/videos appear inverted too — **expected for now**; true-color
   media holes are the v2 milestone on Linux.
4. `Ctrl+Alt+I` → colors snap back; press again → inverted.
5. Multi-monitor: every connected monitor should invert.
6. `Ctrl+Alt+Q` → colors restore and the app exits cleanly.

## Step 4 — the crash-recovery drill

```bash
./invertonator &          # start inverted
pkill -9 invertonator     # simulate a crash — the ramp PERSISTS (expected)
./invertonator --restore  # pushes identity ramps — colors come back
```

## What to send back

- The **full `--self-test` output** (most important).
- Distro and desktop: `cat /etc/os-release` and
  `echo $XDG_CURRENT_DESKTOP $XDG_SESSION_TYPE`.
- GPU: `lspci | grep -iE 'vga|3d'`.
- One line on: text crispness, any flicker, whether all monitors inverted.

---

## If you're on Wayland

Modern Ubuntu/Fedora default to Wayland. Invertonator v1 needs X11:

1. The binary will refuse with a clear message if there is no X display.
2. Log out → click your user → **gear icon** (bottom-right on GNOME) →
   pick **"Ubuntu on Xorg"** (or "Plasma (X11)") → log in → retry.
3. Native Wayland support (KDE KWin invert effect / GNOME extension
   backends) is on the roadmap.

## No Linux machine? (maintainer note)

- **You don't need one.** CI builds the Linux binary on every push; any
  Linux user can test it by following this runbook.
- Want to see it yourself: install VirtualBox on Windows, boot an Ubuntu
  24.04 ISO (live session is enough), choose **"Ubuntu on Xorg"** in the
  login gear menu, then follow Steps 1–4. Inside the VM, `--self-test`
  and the visual inversion both work against VirtualBox's emulated GPU.
- WSLg is **not** representative: it is Wayland/XWayland, and CRTC gamma
  writes are intercepted — the self-test will correctly report
  RAMP-UNCHANGED there.
