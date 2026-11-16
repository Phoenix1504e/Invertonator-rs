# Invertonator-rs worklog (repo-tracked project memory since alpha.18)

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

### Field finding #3 — CORRECTED by journal (alpha.18 analysis)
The 11:15:48 journal lines (alpha.17's own wording + "Invertonator 17 enabled")
PROVE the live enable DID re-import the new module from disk on the
ERROR→enabled transition — no cached-import problem. The stale Extensions-app
dialog (alpha.16 SyntaxError) was the app's UI cache, cosmetic only.
What still needs relogin: REINSTALL over an ACTIVE (loaded) copy — the shell
doesn't hot-reload a working module. install.sh alpha.18 now says both.

### Field finding #4 — CONFIRMED + FIXED in alpha.18 (sandbox **bfb49b3**)
Journal branch: construction threw `TypeError: effect.set_shader_source is not
a function` → effect=null → every toggle no-op. GNOME 51 GJS exposes NO
set_shader_source(). FIX: set GLSL via GObject property 'shader-source'
(stable since Clutter 1.10) with read-back guard. Plus: install.sh honesty
lines, APPLY.md unzip -z freshness gate, versions → 0.3.0-alpha.18, metadata
v18. cargo toolchain absent (sandbox recycle) → Cargo.lock hand-synced
(single version line, zero dep changes; CI builds fresh).
Delivered: zip comment `Invertonator-rs bfb49b3 0.3.0-alpha.18`, mirror
24 files byte-verified. Awaiting: user push + Super+I field report.

### Field finding #5 — RESOLVED in alpha.19 (sandbox **4ba123c**)
Journal: `ClutterShaderEffect has no property named 'shader-source'` → BOTH
classic GLSL routes dead in GNOME 51. Research: gjs.guide "Port to GNOME 51"
— Shell.GLSLEffect removed; Clutter.ShaderEffect takes GLSL from
vfunc_get_static_snippet() -> Cogl.Snippet.set_replace(). Proven pattern
copied from murat-cileli/gnome-shell-extension-shader-effects (whole-desktop
GLSL, tested GNOME 48 + 51): dual-API effect (GLSLEffect ≤49 /
ShaderEffect+snippet 50+), attach to Main.layoutManager.uiGroup (NOT
global.stage), sample via auto-provided cogl_sampler0 (never re-declare).
GLSL: `vec4 c = texture2D(cogl_sampler0, cogl_tex_coord_in[0].xy);
cogl_color_out = vec4(1.0 - c.rgb, c.a);` (bare body, no main()).
enable() journals effect base for one-line diagnostics.
Delivered: zip `Invertonator-rs 4ba123c 0.3.0-alpha.19`, 24 files verified.
Awaiting field report #2.

### FIELD REPORT (alpha.19) — v1 CONFIRMED on GNOME 51
User: "**inversion works no holes tho**". Fedora 45 / GNOME 51 / Wayland:
Super+I flips the whole screen, compositor-level, post-render, web-invisible —
parity with the Windows Magnifier backend achieved. "No holes" = expected:
Linux v1 is global inversion by design; true-color holes are the v2 milestone
and are now UNBLOCKED — the alpha.19 effect machinery (dual-API effect class,
snippet GLSL, per-actor attach) is exactly the plumbing v2 needs (apply the
effect per-window-actor + pointer-pick exemption = F7 analog).
Awaiting: one-line protocol completion (crisp / flicker / reboot persistence).
NEXT: alpha.20 = Linux v2 true-color holes — USER GO received
("lets goon v2"). See Task 19.

CONFIRMED: alpha.17 pushed — user-side SHA **f86bfa5** (308b0cb..f86bfa5 → main;
sandbox tree 11239ef byte-identical, 8-file delta as predicted). install.sh printed
**"enabled LIVE"** — but see finding #3: that only whitelisted; the module stayed
cached-failed from the alpha.16 ERROR, so a relogin was still required. Awaiting
field report: Super+I invert / text crisp / flicker / State: ACTIVE.

## Task 19 — alpha.20: Linux v2 true-color holes (F7 pierce)

### Design (user go: "lets goon v2", 2025-11)
- Windows F7-pierce analog. Hole = the SAME inversion shader attached a SECOND
  time to the picked Meta.WindowActor. Inversion is an involution (1−x): the
  window is painted inverted by its own effect, then painted again by uiGroup's
  global pass → TRUE COLOR. The hole IS the window: moves/resizes/fullscreen
  follow automatically; zero rectangle tracking, zero overlay windows.
  Clutter paints parent effects over the already-painted subtree, so uiGroup's
  pass sees post-hole pixels — that ordering is the whole trick.
- Alternative rejected: hole-rect uniforms in the global shader (per-effect
  uniforms on the GNOME 51 snippet pipeline are unproven, and rect tracking is
  exactly the complexity the actor approach deletes).
- Note: this works on GNOME X11 sessions too (same actor path) — no
  XComposite/XShm needed for GNOME; the X11 design stays for the raw binary.
  KWin/KDE backend unchanged on roadmap.
- UX contract: F7 = toggle hole on TOPMOST window under pointer (schema key
  toggle-pierce, default ['<F7>'], rebindable); F7 with inversion off =
  notification, no-op; inversion OFF clears all holes (else the hole window
  would be the only inverted thing on screen); panel menu pierce/clear items;
  indicator "Inversion: ON · N holes"; Main.notify confirms each pierce with
  window title; window closed with hole on → 'destroy' drops the map entry.
- Topmost pick: frame-rect hit-test over global.get_window_actors()
  (minimized / !showing_on_its_workspace filtered), last hit wins; wrapped in
  sort_windows_by_stacking() when available (try/catch fallback). Journal logs
  wm_class + title per pierce for one-shot forensics.
- Deliberately NOT in alpha.20 (scope discipline, one variable per round):
  AT-SPI2 auto-media holes (UIA analog) → v2.1 (auto-pierce known player
  wm_classes: mpv, totem, vlc, celluloid...); hover-live preview → later.

### Implementation (sandbox, alpha.20)
- extension.js: Indicator +pierce/clear menu items, setState(inverted, holes);
  Extension: _holes Map (Meta.Window → {actor, mw, effect, destroyId}),
  _addBinding helper (both bindings, separate journal per binding), pierce(),
  clearHoles(), _addHole()/_removeHole(), setInverted(false) clears holes,
  disable() removes 'toggle-pierce' + drops map. Effect machinery untouched.
- gschema: + toggle-pierce as ['<F7>']; gschemas.compiled recompiled in-sandbox
  (glib-compile-schemas present). install.sh: F7 mentions in both echo paths.
- metadata v20; Cargo.toml/common.rs/Cargo.lock → 0.3.0-alpha.20 (lock hand-
  synced, cargo toolchain absent again post-recycle; CI builds fresh).
  README: status headline, platform matrix row → ✅ v2 shipped, roadmap,
  hole-transience note. node --check (ESM) on extension.js: SYNTAX OK.
- Delivered: zip `Invertonator-rs <sha> 0.3.0-alpha.20` + full mirror
  byte-verified; APPLY.md rewritten (alpha.20 test protocol, 8 steps).
- Awaiting field report #3: pierce on a real player (mpv/Firefox/totem),
  overlap behavior, drag-follow, clear-all, invert-off auto-clear.

### Risk table for round 6 (pre-registered branches, journal decides)
- hole ON logged, NO visual change → effect on Meta.WindowActor no-ops
  (offscreen/shadow path) → attach to actor.get_surface() (texture actor)
  instead of the WindowActor.
- hole lands on the WRONG window on overlap → sort_windows_by_stacking
  direction flipped → take sorted[0].
- F7 swallowed (laptop media row) → rebind via gsettings; panel item covers.
- JS ERROR on F7 → paste decides; vfuncs untouched (alpha.19-proven).

### FIELD REPORT (alpha.20) #3 — v2 pierce CONFIRMED
User: "**pierce made the whole window a hole tho it snaps on very accurately
to that exact window**". Fedora 45 / GNOME 51 / Wayland: the double-invert
hole on the picked Meta.WindowActor renders that EXACT window true color —
pick accuracy + actor-level effect both proven (risk branches #1 and #2
cleared without journal round-trip). Whole-window semantics = expected
v2.0 scope; element-level video-only holes (Windows UIA analog) = AT-SPI2
discovery = v2.1.
- F7-vs-stale-module finding: first F7 test hit Firefox caret browsing —
  live module was still alpha.19 (Version: 19 + old description in
  `gnome-extensions info`; "No such schema" from plain gsettings CLI is
  expected — extension schemas aren't in the CLI's scan path; the && chain
  died there). Disk check + relogin resolved. LESSON: `gnome-extensions
  info` reads the LIVE module — version there can lag disk after reinstall.
Open protocol items: F7 direct post-relogin, drag-follow, overlap-above,
clear-all, invert-off auto-clear. NEXT: alpha.21 = v2.1 AT-SPI2 auto-media
holes (pending user go).

## Task 20 — alpha.21: pierce hotkey → Super+F7 (browser-safe) + registration logging

### Field report chain (alpha.20)
- User: "**pierce made the whole window a hole tho it snaps on very accurately**"
  (menu pierce = mechanism PROVEN) then "**the f7 honestly just goes directly
  to the browser but the manual click button works**".
- First F7 test during alpha.20 hit Firefox caret browsing — live module was
  still alpha.19 (Version: 19 + old description in `gnome-extensions info`;
  plain-gsettings "No such schema" is EXPECTED — extension schemas are not in
  the CLI scan path). Disk check + relogin flipped the module to alpha.20.
- With alpha.20 LIVE (menu pierce works), F7 STILL reaches Firefox ⇒
  decisive deduction: Super+I is consumed by mutter, so the grab pipeline
  works; a registered `toggle-pierce` would consume F7 before delivery.
  It didn't ⇒ the unmodified-F7 registration/grab is the failure — and plain
  F7 is browser-claimed (caret browsing) regardless, so a working grab would
  still be a foot-gun. Journal paste never arrived; shipped full-branch
  coverage instead (each branch either fixed or now self-logging).
- Also field-observed: re-running an OLD scrollback apply block (alpha.6
  commit message) → chain stops harmlessly at "nothing to commit, working
  tree clean" → confirms && -chain guards stale blocks AND that user tree ==
  zip tree (575514a user sha = 8b6bc44 sandbox tree).

### alpha.21 changes (sandbox; awaiting push)
- schema: toggle-pierce default `['<F7>']` → `['<Super>F7']` + description
  documents WHY (browsers claim F7; unmodified-F7 grab doesn't consume).
  gschemas.compiled recompiled; key presence verified in sandbox.
- extension.js `_addBinding`: journals `keybinding '<name>' registered
  (action=…, accel=…)` on success — silent misses are now impossible; a
  failed get_strv (stale schema) throws INTO the same catch with the reason.
- install.sh: post-compile verification loop — grep every hotkey key in the
  installed gschemas.compiled, WARN loudly on absence.
- metadata v21; Cargo trio → 0.3.0-alpha.21; README (status headline,
  matrix hotkeys, alpha.21 hardening paragraph); install.sh echo lines →
  Super+F7; APPLY.md rewritten (alpha.21 protocol includes the two
  "registered" journal lines as the acceptance gate).
- Delivered: zip `Invertonator-rs <sha> 0.3.0-alpha.21` + mirror byte-verified.
- Risk table for round 7: registered-line absent → schema/source problem
  (paste decides); registered with accel=["<Super>F7"] but Super+F7 no-op →
  mutter grab failure on Super+F-keys (unprecedented; would demand journal);
  everything green → v2.1 AT-SPI2 auto-media is next (user go pending).
