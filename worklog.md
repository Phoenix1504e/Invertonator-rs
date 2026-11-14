# Invertonator-rs worklog (sandbox-local, untracked)

## Task 18 — alpha.17: extension import crash fix + installer enable fallback

### Field report chain (Fedora 45 / GNOME 51 / Wayland)
1. alpha.16 (sandbox cdd23cd → user push **308b0cb**) delivered; install.sh ran pre-login.
2. Post-login `gnome-extensions info`: Version 16 recognized, **Enabled: No / INITIALIZED**
   — root cause: install.sh enable ran before login; shell had not scanned the dir;
   `2>/dev/null || true` swallowed the failure. Manual `gnome-extensions enable` given.
3. User: **"nothing happened"**. Then: **"it shows active in gnome extensions"** (app
   toggle reflects the whitelist only).
4. **Root cause found in alpha.16 code** (308b0cb): extension.js used DEFAULT imports
   for shell ui modules —
   `import Main from 'resource:///org/gnome/shell/ui/main.js'` — but main.js /
   panelMenu.js / popupMenu.js export named bindings only. On enable the shell tried
   to import → `SyntaxError: doesn't provide an export named 'default'` → enable()
   never ran → no icon, no keybinding, no inversion = literally "nothing happened".
   (Pre-enable INITIALIZED is metadata-only; import happens on first enable.)

### alpha.17 fix (sandbox commit **11239ef**, NOT yet user-pushed)
- extension.js: canonical `import * as Main from ...` namespace imports; ShaderEffect
  enum lookup hardened (`Clutter.ShaderEffectType?.FRAGMENT_SHADER ??
  Clutter.ShaderType?.FRAGMENT_SHADER`); shader body unchanged (proven
  texture2D/cogl_color_out pattern). ESM syntax verified with node --check.
- install.sh: when live enable fails (pre-login install), whitelists uuid in
  `org.gnome.shell enabled-extensions` via gsettings (handles `@as` quirk, prunes
  disabled-extensions) → next login comes up ACTIVE without manual enable.
- metadata version 17, shell-version 45–51; Cargo.toml/common.rs → 0.3.0-alpha.17;
  APPLY.md rewritten (&&-chained, path-independent); README status updated.
- Delivered: download/Invertonator-rs-full-source.zip (30 entries, comment
  `Invertonator-rs 11239ef 0.3.0-alpha.17`) + full-tree mirror in
  download/invertonator-changed-files/ (23 files, byte-verified vs HEAD).
- cargo check linux target: clean.

### Expected post-apply verification (user side)
- Brightness icon in top-right panel; Super+I inverts whole screen incl. panel;
  `gnome-extensions info` → State: ACTIVE; journal `Invertonator 17 enabled`.
- Note: enabling alone still inverts nothing (start-enabled=false by design) —
  the panel icon is the visible sign of a healthy enable; Super+I does the invert.

### Queued
- **Placeholder recurrence (field finding #2)**: chat command block contained literal
  `<path-to-zip>` → user pasted verbatim → unzip failed silently down the chain
  (no commit, no push, old install.sh re-ran). RULE going forward: every user-facing
  command block must be 100% literal (~/Downloads default) + `unzip -z` freshness
  gate first (same filename across versions = stale-zip footgun). alpha.18: fold
  this gate into APPLY.md.
- Linux v2: true-color holes — Wayland per-window-actor inversion + pointer-pick
  exemption (F7 analog); X11 XComposite+XShm overlay.
- Windows side (parked, not dropped): alpha.15 hysteresis field test,
  "video a little slow in the hole", YouTube hole stability; diagnostics
  auto-delivery discipline unchanged.
- KDE/Wayland KWin script backend (roadmap).

### Field finding #3 — stale ERROR cache (post-alpha.17 install)
Extensions app dialog after successful reinstall: SyntaxError at extension.js
**line 38 col 8** — that is alpha.16's `import Main` token (alpha.17 line 38 is a
comment). Running shell cached the failed import from the earlier manual enable;
disk replacement does NOT hot-reload an ERROR-state module. Toggle showed OFF
(error dialog likely triggered a disable). Fix = re-whitelist + relogin.
LESSON: after reinstalling over an ERROR-state extension, relogin is MANDATORY;
`enabled LIVE` message is misleading there (it only whitelists, module import may
stay cached-failed).

### Field finding #4 — alpha.17 loads; shader no-op suspected
User relogged, saw indicator + "Inversion: OFF" menu item (proof: enable() ran,
import bug dead). Super+I AND menu click → no visible inversion. Fault domain
narrowed to the effect path. Branch table (awaiting journal):
- `Invertonator 17 enabled` present → alpha.17 live ✓
- `Invertonator: inversion ON` + no visual → effect added but stage-level shader
  no-ops on GNOME 51 → alpha.18 mechanism switch: same ShaderEffect on a dedicated
  fullscreen OVERLAY ACTOR instead of global.stage (actor-level effects =
  battle-tested path, cf. Blur My Shell; stage is the special-cased root actor).
- `no shader effect — cannot invert` → construction threw (enum/set_shader_source)
  → targeted fix in makeInvertEffect.
- `effect toggle failed (...)` → add_effect threw → targeted fix.
- Cogl/shader compile errors → GLSL fix.
- No Invertonator lines → module not loaded at all (revisit whitelist/relogin).
Discipline: no blind patching until journal says which branch.

CONFIRMED: alpha.17 pushed — user-side SHA **f86bfa5** (308b0cb..f86bfa5 → main;
sandbox tree 11239ef byte-identical, 8-file delta as predicted). install.sh printed
**"enabled LIVE"** — but see finding #3: that only whitelisted; the module stayed
cached-failed from the alpha.16 ERROR, so a relogin was still required. Awaiting
field report: Super+I invert / text crisp / flicker / State: ACTIVE.
