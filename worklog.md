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

## Task 21 — alpha.22: v2.1 "holes everywhere" (manager, fullscreen video auto-hole, survival, scanout guard)

### Field report chain (alpha.21)
- User: "**yea both keys work**" — Super+I AND Super+F7 both confirmed live on
  Fedora 45 / GNOME 51 / Wayland. Round-7 risk branches all cleared: the
  registered-line journaling + Super+F7 rebind fixed the last dead branch; no
  journal paste needed.
- User: "**lets try to make the holes work per tab, per window and holes on
  video and things**" — the v2.1 go signal.

### Scope decision
- "Per tab": a browser window is ONE buffer to the compositor — tabs are web
  content inside it. A window hole covers whichever tab is showing and follows
  tab switches for FREE (nothing to implement; documented honestly). Element-level
  holes (video element only, chrome stays dark) need AT-SPI2 rects — deferred to
  v2.2/alpha.23, not attempted this round (needs its own field evidence loop).
- "Per window": holes were already multi + per-actor; what was missing is
  VISIBILITY and individual control → hole manager submenu.
- "Video": the fullscreen heuristic ships NOW (UIA auto-media analog, heuristic
  edition) — fullscreen video is the dominant video experience and the window
  state is compositor-visible. Native players = manual Super+F7 (proven). PiP =
  manual pierce (pick includes OR windows now).
- Windows side untouched (alpha.15 code, version bump only).

### alpha.22 changes (sandbox; awaiting push)
- extension.js:
  - Indicator: `Holes: N` PopupSubMenuMenuItem — one row per hole
    ("title — class (auto)"), click a row closes exactly that hole;
    rebuildHoles() live-updates on every hole mutation via _refreshHoleUI().
    Pierce menu item label now renders the LIVE accel from settings
    (accelFor() pretty-printer) — no more stale "(F7)" text.
  - Fullscreen auto-hole: every window watched (existing at enable +
    window-created stream) via notify::fullscreen; while inverted and
    auto-hole-fullscreen=true, a window entering fullscreen is pierced
    automatically (auto=true hole, "Auto-hole ON: title" notify); leaving
    fullscreen removes ONLY auto holes — manual holes never auto-removed.
    setInverted(true) syncs holes for already-fullscreen windows.
  - Hole survival: actor destroy → idle re-check (GLib.idle_add) →
    mw.get_compositor_private() non-null and different → reattach fresh effect
    to the recreated actor; else drop + disconnect unmanagedId. Every hole
    also carries an 'unmanaged' safety net on the Meta.Window.
  - Direct-scanout guard: _setUnredirectDisabled(on) while inverted —
    global.compositor.disable_unredirect/enable_unredirect first, then
    Meta.disable/enable_unredirect_for_display fallback, else honest journal
    line (uiGroup offscreen effect already forces compositing; this is
    belt-and-braces for fullscreen video direct scanout).
  - Pick: OR windows (context menus, tooltips) skip the workspace check
    (is_override_redirect) — pierceable now.
  - removeHole() made public (manager rows + hotkey paths share it);
    gschema adds auto-hole-fullscreen (b, default TRUE — user asked for it).
- install.sh: schema-check loop now verifies 4 keys (incl. auto-hole-fullscreen);
  auto-hole lines in both echo paths. metadata v22. Cargo trio →
  0.3.0-alpha.22. README (status headline, matrix row, session-transient note,
  roadmap v2.1 shipped / v2.2 AT-SPI2). APPLY.md rewritten (alpha.22, 7-step
  protocol incl. per-tab switch, 2-window manager, YouTube fullscreen
  auto-hole, manual-hole persistence). Sandbox gates green: node --check OK,
  glib-compile-schemas --strict OK, all 4 keys present in compiled schema,
  metadata JSON parses, install.sh bash -n clean.

### Risk table for round 8 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| R1: notify::fullscreen absent/never fires for some window types | no "Auto-hole ON" line on YouTube fullscreen | journal paste decides; fallback = re-check in size-changed, or poll at 1Hz while inverted |
| R2: false-positive auto-holes annoy (games/fullscreen editors holed too) | user wants fullscreen terminal NOT true color | gsettings auto-hole-fullscreen false (documented); or v2.2 AT-SPI2 gates auto-hole to real media |
| R3: disable_unredirect API mismatch on GNOME 51 | "no unredirect API" journal line (honest, harmless) | none needed — insurance only; uiGroup effect already forces compositing |
| R4: manager rows leak/duplicate | duplicate rows or stale rows after window close | listHoles is rebuilt from the live Map on every mutation; unmanaged drops entries — journal shows "hole dropped" |
| R5: reattach loop on rapid actor churn | repeated "hole reattached" lines | reattach only fires from a destroy of the CURRENT hole actor; guarded by map identity check |
| Everything green | all 7 protocol steps pass | next = v2.2 AT-SPI2 element rects (video-element-only holes), user go pending |

Delivered: zip `Invertonator-rs <sha> 0.3.0-alpha.22` + changed-files mirror byte-verified.

## Task 22 — alpha.23: v2.2 element media lock-on (AT-SPI2 image/video rect holes)

### Field report chain (alpha.22)
- User: "**can we implement auto hole where it only locks on to images and videos
  and rest everything is just inverted**" — the v2.2 go signal, exactly the
  AT-SPI2 rect-hole milestone the roadmap reserved after alpha.22's
  fullscreen heuristic.

### Scope decision
- Element holes need RECT holes, not window holes. Two candidate machineries
  were weighed: (a) rect-masked global shader via uniforms — REJECTED: uniform
  API on GNOME 51 is unverified (alpha.17/18 shader-API archaeology cost two
  rounds), texcoord→screen mapping breaks under overview/monitor transforms;
  (b) clipped Clutter.Clone of the window actor carrying the invert effect —
  CHOSEN: reuses the PROVEN involution machinery, zero new shader APIs,
  z-inserted above the source window actor so stacking stays correct
  (overlapping windows cover the hole — Magnifier-analog semantics).
- Identity-overlay math (clone translation = −container position) makes the
  hole content scale-INVARIANT: the clone always re-paints the window exactly
  where the real window paints; the clip alone selects the rect.
- Media discovery = AT-SPI2 tree walk of the FOCUSED window's app (PID match,
  wm_class name fallback): roles IMAGE/VIDEO/ANIMATION ≥48px. Budgeted
  (240 D-Bus calls/walk), chunked (8ms idle slices), cached (re-walks cost
  ~0 calls for unchanged subtrees), events (bounds-changed → cheap extents
  refresh of live holes; document/children events → cached re-walk), hung-app
  blacklist 60s, pool cap 6, focus change = context gone → holes cleared
  (alpha.11 evidence-rule analog).
- Measured calibration: k = frame_rect.width / frame-extents.width per frame;
  logical(p) = fr.xy + (p − frameExt.xy)·k. Kills HiDPI/scale mismatch WITHOUT
  assumptions; the Δ between element and frame cancels window movement between
  measurement and publish.
- Atspi import is DYNAMIC (await import('gi://Atspi')) — a static import
  failure would kill the whole module (alpha.16 lesson); now it only costs
  the lock-on feature. Every AT-SPI API access is ?.() guarded with a
  one-shot distinctive journal line (`AT-SPI API miss '...'`).
- toolkit-accessibility: NEVER auto-flipped; journaled hint when OFF
  (documented command in APPLY §3). Chromium/Firefox/GTK trees usually
  surface once an AT-SPI client (us) is present.
- Window-hole interplay: windows carrying a manual/auto window-hole are
  excluded from element publish AND their element holes dropped on pierce
  (a rect on a true-color window would RE-INVERT it).

### alpha.23 changes (sandbox; awaiting push)
- extension.js: media state block + budgets; _mediaInitAtspi (dynamic import,
  role sanity); _mediaConnectEvents (5 event types, deregistered on disable);
  _mediaWalk/_mediaResolveFrames/_mediaWalkChunk (budget/cache/chunk/
  blacklist); _mediaPublishFromFound (area sort, cap 6, frame clip, min
  edge); _mediaPublish (identity+proximity diff, churn-free updates);
  _elementHoleCreate/Update/Reposition/Destroy (container+clone+effect,
  position-changed/size-changed tracking); _mediaRefreshExtents (scroll
  follow, ≤12 calls); _mediaDropForWindow; focus watcher (400ms debounce);
  toolkit hint; indicator shows `media lock-on: N rects (auto)` row.
- gschema: + auto-hole-media (b, default true). install.sh: 5-key schema
  check + lock-on echo lines. metadata v23 + description. Cargo trio →
  0.3.0-alpha.23. README (status headline, matrix, roadmap v2.2 shipped /
  v2.3 hole-quality). APPLY.md rewritten (alpha.23, 6-step protocol with
  AT-SPI2 journal line + toolkit-accessibility branch). Sandbox gates green:
  node --check OK, glib-compile-schemas --strict OK, all 5 keys in compiled
  schema, JSON parses, install.sh bash -n clean.

### Risk table for round 9 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| R1: gi://Atspi import fails in shell process | `unavailable (Atspi import: …)` journal; rest of extension fine | confirm import mechanics; fallback = out-of-process helper via bus |
| R2: AT-SPI API variance (get_process_id/getComponent/get_extents) | one-shot `AT-SPI API miss '…'` line names the exact method | adapt call path per line |
| R3: app trees dormant (toolkit-accessibility off / browser a11y asleep) | walks return 0, hint line present | user runs the documented gsettings; re-test |
| R4: rect misalignment (deco/scale mismatch beyond k) | screenshot: holes offset/sized wrong | adjust anchor (frame→buffer rect) or per-frame deco offsets |
| R5: shell jank during walks | UI stutter at 3s cadence | tighten budgets/chunks; blacklist heavier |
| R6: clone/effect GPU cost | stutter only with many holes | cap 6 → 4; reuse one offscreen |
| R7: role coverage gaps (CSS-background videos, canvas players) | media plays but no rect | v2.3: ANIMATION+AUDIO, IMG ancestor capture, or screenshot-diff heuristic |
| Everything green | rects lock, follow scroll/tabs, clear on focus loss | next = v2.3 quality pass, user go pending |

Delivered: zip `Invertonator-rs <sha> 0.3.0-alpha.23` + changed-files mirror byte-verified.

## Task 23 — alpha.24: v2.3 auto-hole fix (rect-shader element holes, event capture, priority repair)

### Field report chain (alpha.23)
- USER: "auto hole doesnt woks" — alpha.23 (zip ddd5aa2) on Fedora 45 / GNOME 51. No install-side issue (schema keys present, installer intact). Sandbox-side code audit found three concrete bugs:
  1. KILLER: element rect holes were Clutter.Clone overlays whose translation anchored cloned window content at stage (0,0) — aligned ONLY for windows already at the origin (fullscreen on the primary monitor), shifted window copies for every windowed app. Risk-table R4 was real but the magnitude was "completely wrong anchor", not deco-offset drift.
  2. Walker starvation: budget 240 D-Bus calls (~120 nodes) per 3s walk vs multi-thousand-node web trees; cache/seen keyed by Atspi.Accessible object identity (libatspi wrapper reuse unguaranteed) → frontier could stall forever (risk R2/R3 adjacent).
  3. Priority inversion: fullscreen whole-window auto-hole suppressed element rects (entries filtered for holed windows) → chrome true color in fullscreen, contradicting the v2.2 ask.

### alpha.24 changes (sandbox; awaiting push)
- RENDERING REWORK: element hole = ONE rect-invert Clutter.ShaderEffect per window; GLSL bakes rects as literal normalized comparisons (no uniforms — Cogl GI uniform surface unreliable), inverts ONLY inside; global pass re-inverts → media true, chrome stays inverted. Rects normalized against the actor's paint-volume box (get_paint_volume → get_paint_box → allocation fallback) = exactly the bounds Cogl's offscreen texture spans → shadows/CSD margins cancel; effect rides the actor → move/resize follow free. Clone overlay, container, position-changed/size-changed repositioning all DELETED.
- DISCOVERY: budget 480; free event capture (Atspi event sources probed 16/s for media roles → eventHits cap 24 TTL 60s; windows matched by rect-center containment — works when pid/name matching fails); event publishes debounce 700ms through the refresh path.
- PUBLISH MODEL: complete=true (walk finished or clean-empty) = authoritative replace + fallback re-arm; complete=false (event/refresh/starved) = supplemental update/create, never wipes; starved walks publish event-only (no [] wipe → no flip-flop). Rebake only when a rect moved ≥6px or count changed.
- PRIORITY: complete walk proving rects removes an auto fullscreen hole (swap journaled); complete walk proving no rects re-arms the whole-window hole (native players/games); _onFullscreenChange skips the whole-window hole when rect holes exist; setInverted(true) delays _syncFullscreenHoles 1.6s when the walker runs.
- A11Y: toolkit-accessibility SET (not hinted); GTK3-restart caveat journaled. DIAGNOSTICS: walk summaries (30s throttle), rect bake anchor one-shot per app class, no-app-match one-shot, swap/fallback lines; LINUX-TESTING.md documents the paste-back grep + got the missing v2 holes test protocol.
- DOCS/CONFIG: metadata v24, Cargo VERSION alpha.24, gschema description documents priority/fallback, install.sh note updated, README (status block alpha.24 / matrix v2.3 / roadmap), APPLY.md rewritten (freshness gate alpha.24; 7-step protocol incl. move-window glue test + fullscreen-chrome-stays-inverted test).
- VALIDATION: node --check green; glib-compile-schemas --strict green; all 5 keys in compiled schema; bash -n green; JSON/XML green. Cargo NOT run (no rustup in this sandbox session; only Rust change = VERSION string, zero functional risk; CI compiles).
- PROCESS LESSON: MultiEdit here applies edits non-atomically on anchor failure (earlier edits stuck) — a grep caught the duplicated rect-effect block; dedup + one-edit-per-call after that. Always grep for duplicate definitions after a failed multi-edit.

### Risk table for round 10 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| R1: paint-volume ≠ offscreen bounds (rect misalignment) | `rect bake "class" actor@… box=…` journal + shifted-hole screenshot | switch normalization anchor to paint box/allocation; compare box vs actor size |
| R2: Atspi wrapper identity differs per fetch (cache useless) | `a11y walk … media=0` on media-heavy pages, cache=… stays tiny | cache keyed by role+name+index path strings; rely on event capture meanwhile |
| R3: trees dormant (toolkit-accessibility just set, apps not restarted) | walk media=0 + `toolkit-accessibility was OFF — set ON` line | restart the app once; re-test |
| R4: gi://Atspi import fails in shell process | `unavailable (Atspi import: …)` | out-of-process helper via bus (fullscreen auto-hole unaffected meanwhile) |
| R5: rebake churn jank (scroll storms recompile shaders) | stutter while scrolling with holes visible | raise MEDIA_REBAKE_DELTA_PX; bake→uniform path only if Cogl GI allows |
| Everything green | windowed rects glue to media; fullscreen = video true + chrome inverted; native player = whole window | v2.3 quality pass done; next = pooling beyond 6, telemetry |

Delivered: zip `Invertonator-rs <sha> 0.3.0-alpha.24` + changed-files mirror byte-verified.

## Task 24 — alpha.25: walker convergence + event-API hardening (field round 10 follow-up)

### Field report
- USER: "still the holes are not automatic" — immediately after a session where the WINDOWS PowerShell apply block had been pasted into Fedora bash (`cd C:\Users\admin\invertonator-push` → no such file, `Expand-Archive: command not found`, git "nothing to commit / up-to-date" — the unzip/commit/push never ran). So alpha.24 was most likely never installed; the reply must lead with the version check (`gnome-extensions info` + `unzip -z` gate) and the correct LINUX block.
- Sandbox audit found REAL alpha.24 weaknesses that would keep element holes from ever appearing even on a correct install — fixed before shipping alpha.25:

### alpha.25 changes
- CONVERGENCE (the big one): the walker queue now CARRIES OVER between ticks for the same target window. alpha.24 restarted the BFS from the roots every 3s walk; with the cache missing (wrapper identity unguaranteed, worklog R2) the same first ~480 nodes were re-explored forever and a video deep in a big web tree was NEVER reached. alpha.25: 480 fresh nodes per tick, `complete=true` publish when the queue finally drains (holes appear), starved walks journal a throttled "a11y walk converging — queue=N media=M" line so progress is visible in the field log.
- RE-ROOT: tree-shape events (document:load-*, children-changed) set `reRoot`, which discards the carried queue at the end of the in-flight walk (a stale queue would otherwise keep walking the OLD page and never enqueue the new content). Slow-app aborts also reset the walk state.
- CACHE KEYING: probed once on a real node — `get_path()` string keys when the GI surface exposes it (stable across D-Bus re-fetches), wrapper identity otherwise; journaled as `key=path|wrap` in every walk line. `hits=` added to the walk diag so cache effectiveness is measurable from the field.
- EVENT API FALLBACK: registration tries `EventListener.add_callback(cb, type)` then `EventListener.new(cb)` + `listener.register(type)`; the used API and listener count are journaled ("event capture on via … (N/5 listeners)" / "event capture OFF"). Shutdown deregisters via remove_callback / deregister_callback / listener.deregister — all best-effort.
- BAKE DIAG: when ALL rects clip out during bake (paint-volume-mismatch signature, risk R1), a one-shot per window class journals the box + actor origin and asks for the paste-back.
- Versions: metadata 25, Cargo/common.rs 0.3.0-alpha.25. No schema, shader, or publish-priority changes — alpha.24's rect-shader rendering stands.

### Risk table for round 11 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| C1: user never installed alpha.24/25 (Windows block again) | `gnome-extensions info` Version < 25 or `unzip -z` gate mismatch | correct LINUX block, nothing else |
| C2: convergence works, media found late | "a11y walk converging" lines → then "media lock-on N rect(s)" within ~15-30s on huge pages | acceptable; tune tick cadence if reported slow |
| C3: tree never exposes media roles | walk lines show media=0 forever + event capture on; browser pages with obvious images | browser a11y disabled → document Chromium --force-renderer-accessibility / Firefox about:config parity; app-level, not fixable in-shell |
| C4: no AT-SPI app match (pid/name) | "no AT-SPI app for pid=… class=…" one-shot | event capture covers; if also dead, out-of-process helper (R4 path) |
| C5: bake-clip line appears | holes found but nothing punches | paint-volume anchor rework (R1 path) |
| Everything green | windowed rects glue to media; fullscreen = video true + chrome inverted; native player = whole window | v2.3 converged; next = pooling beyond 6, telemetry |

## Task 25 — alpha.26: re-root thrash fix (journal round 11) + host-side diag tool

### Field data (first useful journal paste of the whole project)
- USER journal: alpha.25 confirmed RUNNING (`Invertonator 25 enabled`), event capture alive via the `EventListener.register` FALLBACK (`Atspi.EventListener.add_callback` does not exist on Fedora 51's GI surface — the alpha.25 fallback design was load-bearing), cache keying `wrap` (no get_path), carry-over active (`carried=199`).
- THE SMOKING GUN: `media=0` on every walk, and `carried=199` (19:03:51) followed by ALL-fresh walks (`carried=0`, queues 146/187/17/202). Root cause: alpha.25 set `reRoot` on EVERY `object:children-changed`; YouTube emits them CONTINUOUSLY while playing (progress/buffering/DOM churn) → the walk re-rooted inside the browser chrome every tick and NEVER reached the content document. alpha.25's convergence fix was defeated by its own reRoot policy — i introduced this bug in the same release that added the carry-over.
- Manual Super+F7 holes work on the same window (librewolf) → shader machinery fine; discovery is the only broken layer.
- SECOND open question: LibreWolf is a hardened Firefox fork that ships `accessibility.force_disabled=1` (a11y-tree fingerprinting mitigation) — if the content tree is closed to ANY client, NO walk change can help. Needs the host-side probe to decide (extension cannot fix browser prefs).

### alpha.26 changes
- `_mediaOnEvent`: reRoot ONLY on `document:load-complete/stopped` (page load = genuinely new tree). `children-changed` keeps only the schedule acceleration. (R11 fix)
- `_mediaWalk` steady-state guard: drained queue + same target + no reRoot + <MEDIA_REROOT_MIN_MS(8s) since last root → cheap extents refresh instead of re-root; fresh revalidation walks bounded to once per 8s. `lastRootTs` state added.
- `_mediaWalkChunk` DOCUMENT PRIORITY: DOCUMENT/DOCUMENT_WEB/DOCUMENT_FRAME/DOCUMENT_* kids are unshifted (sibling order preserved via reversed iteration) → the walk dives into content docs as soon as they appear instead of drowning in chrome/sidebar siblings. `m.docRoles` built at AT-SPI init.
- Converging diag line now prints app name + `hits=` (cache effectiveness pre-drain — R2/R3 verdict data).
- `invertonator-diag.sh` (repo root, also shipped standalone): sections 1-6 — running-vs-installed version, session, toolkit-accessibility, browser pref greps (user + system cfg, native + flatpak paths, conditional force_disabled verdict), independent python/gi AT-SPI probe (per-app node counts, DOCUMENT-node counts, media counts, sample extents, verdict banner), journal tail.

### Risk table for round 12 (pre-registered)
| Branch | Evidence | Verdict path |
|---|---|---|
| D1: thrash fixed, tree open | converging lines show carried>0 persisting → drain → "media lock-on N rect(s)" within ~10-30s | DONE — v2.3.2 quality pass |
| D2: thrash fixed but probe shows media=NONE for LibreWolf | diag section 5 verdict NO + document-nodes=0 or missing app | user flips accessibility.force_disabled=0 in about:config (LibreWolf default) + browser restart; extension side complete |
| D3: probe sees media, extension walk still media=0 | diag YES + journal media=0 with carried progressing | matching/walk bug — probe pid/name table vs extension app match; next code round targets _mediaResolveFrames |
| D4: cache hits=0 forever (wrap mode unstable) | converging lines hits=0 across ticks | walk still converges via carry-over; consider cache by (parent-path,index) composite keys |
| D5: bake-clip line | rects found but nothing punches | paint-volume anchor rework (R1 path, unchanged) |

## Task 26 — alpha.27: stored area pins ("select an area once, it gets stored")

### Field context
- USER accepted the current auto-hole state ("this is probably fine tbh") and asked for a SELECTION PROMPT that stores an area ONCE — "and it gets stored for youtube as it doesnt change". Directly addresses the discovery problem from rounds 10-12 WITHOUT touching the walker: where the AT-SPI tree is closed (LibreWolf hardened builds, round-11 finding) a user-selected stored rect is the only discovery that can work.
- Design constraint taken from the user's own words: YouTube's layout does not change relative to the window → store the rect NORMALIZED to the focused window's frame (rel 0..1), not absolute pixels — window moves and resizes follow, other videos on the same site match too.

### alpha.27 changes (v2.3.3)
- SCHEMA: new keys `pin-hole` (as, default `['<Super>F8']`) and `pinned-holes` (s, JSON array, default '[]'); compiled schema shipped updated; install.sh now verifies SIX keys.
- SELECTION (`pinSelectStart`, Super+F8 / panel menu "Pin a stored area"): fullscreen St.Widget ABOVE the stage (outside uiGroup — the overlay itself is NOT inverted), BindConstraint to the stage, modal grab via Main.pushModal (handle kept; legacy popModal fallback), pointer draw (dashed blue rect), Esc/right-click cancel, commit on release. Selection smaller than 24px or 2% rel → nothing stored. Requires inversion ON and a focused non-OR anchor window.
- STORAGE: the rect is stored rel to the anchor window's frame (4 decimals) + match tokens from the title: split on — – | " - " separators, drop app-name segments (PIN_APP_TOKENS: browsers + players), keep up to 4 tokens 4-48 chars. "Song X - YouTube — Mozilla Firefox" → ["song x","youtube"]. Tokens empty → class match fallback. Dedupe identical stores; cap 16 (oldest evicted); persisted in gsettings — survives reboots.
- MATCHING: title-first (ANY token contained in the window title — a different video still matches via "- youtube"), class fallback otherwise. Scan = 2s timer + focus-change + window-created hooks, pure string ops, no D-Bus.
- APPLY: one rect-shader hole per matching VISIBLE window at the stored relative position (same buildRectFragment machinery as media rects; rides the actor — moves free, resizes re-bake via size-changed). Fullscreen matching windows are skipped; a pinned window that GOES fullscreen drops its pin (windowed layout is meaningless there) and the fullscreen auto-hole/media rects take over; un-fullscreening restores the pin.
- PRIORITY: manual Super+F7 pierce > stored pins > media rects > fullscreen auto-hole. `_mediaPublish` skip() now includes `_pins`; the fullscreen fallback refuses pinned windows; `_addHole` (manual/auto whole-window) drops the pin and removeHole restores it.
- UI: panel menu gains "Pin a stored area", a "Stored areas: N" submenu (one row per pin: title tokens/class + rel coords, click forgets that entry) and "Forget all stored areas"; rebuildPins is signature-guarded (the 2s tick must not churn the open menu).
- DIAG: invertonator-diag.sh section 1 now prints the stored pins.
- Versions: metadata 27, Cargo/common.rs 0.3.0-alpha.27. Walker/walker-priority code otherwise untouched — alpha.26's re-root fix stands.

### Risk table for round 13 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| P1: pin works end-to-end | Super+F8 drag → `area stored — match title tokens [song x, youtube]` journal → close window → reopen YouTube → `stored area applied` within ≤2s, rect true color at the same relative spot | DONE — v2.3.3 quality pass |
| P2: grab API mismatch (popModal/pushModal shape on 51) | journal `selection grab failed` or a stuck overlay/input after Esc | overlay.destroy() already releases grabs (Clutter auto-dismiss); adjust teardown if a stuck grab is reported |
| P3: rel drift on browser UI scaling (fractional HiDPI) | stored rect lands slightly off the player after window resize | bake diag line (`rect bake`) vs screenshot; consider storing rel against the CONTENT rect (window minus shadow) instead of frame rect |
| P4: false-positive matches (token too generic) | pin appears on non-YouTube windows (e.g. token = short common word) | submenu row shows tokens; user forgets the entry; consider requiring ≥2 tokens matched when title has >2 segments |
| P5: fullscreen pin conflict | fullscreen shows a mis-shapen true rect | should be impossible (pins dropped on fullscreen) — if seen, the fullscreen check missed a path; check `stored area dropped (window went fullscreen)` line |
| Everything green | pinned YouTube window: instant true rect, zero walk needed; media walk irrelevant on that window | v2.3.3 converged; next = per-monitor pin variants, Windows-side analog (F8 UIA rect store) |

## Task 27 — alpha.28: pin selection adjust + confirm step

### Field context
- USER: "okay so after selection give them a chance to readjust and then confirm the area" — alpha.27's commit-on-release was too eager: one sloppy drag and a wrong rect is already stored (then has to be found in the "Stored areas" submenu and forgotten). The readjust step is also the natural place to catch the P3/P4 risks (off-by-a-bit rects from fractional scaling, misclicks) BEFORE they persist.

### alpha.28 changes (v2.3.4)
- Two-phase state machine in the selection overlay: `drag` → (release) → `adjust` → (Confirm) → commit; plus transient `resize`/`move`/`redrag` phases. The alpha.27 commit path (`_pinSelectCommit`, rel math, tokens) is byte-for-byte unchanged — only gated behind the new confirm.
- ADJUST CHROME (built once, reused): 8 resize handles (4 corners + 4 edges, 13px squares) positioned from one `_pinSyncChrome` pass; a live `W × H px` size chip (non-reactive, floats above the rect, falls inside when the rect hugs the screen top); a Confirm / Redraw / Cancel bar that follows the rect (below by default, above when the rect reaches the screen bottom, edge-clamped).
- INTERACTIONS: handle drag = resize (mask-driven, cursor per handle via `Meta.Cursor.NW/N/NE/E/SE/S/SW/W_RESIZE`, restored to DEFAULT on release/teardown); drag inside the rect = move; drag outside = fresh redraw — a too-small outside drag restores the previous rect instead of losing it (the only asymmetry vs the first drag, where too-small still cancels: at step 1 there is nothing to restore).
- KEYBOARD: Enter (Return/KP_Enter/ISO_Enter) confirms, R redraws, Esc cancels — in every post-drag phase. Toolbar buttons are made non-reactive during live drags so a click can never swallow the release event the drag needs (and a stray press mid-drag self-heals by finalizing at that point).
- MIN-SIZE: resize clamps keep the rect ≥ PIN_MIN_EDGE (24px) throughout the adjust stage, so Confirm can only ever see a valid rect; the too-small guard in `_pinConfirm` remains as a belt-and-braces (stay in adjust, notify).
- Versions: metadata 28, Cargo/common.rs 0.3.0-alpha.28. Walker, pins storage, matching, priority — untouched from alpha.27.

### Field event (round 14 — the paste failure, structural fix)
- The WINDOWS PowerShell block was pasted into Fedora bash AGAIN (`cd C:\Users\admin\invertonator-push` → `No such file or directory`, `Expand-Archive: command not found`, then `nothing to commit, working tree clean` / `Everything up-to-date`). Diagnosis: NOTHING applied, NOTHING harmed — the zip was never extracted (no Expand-Archive in bash) and git refused to commit a clean tree; the alpha.6-era message in the paste never landed. alpha.27→28 gap on origin confirmed untouched.
- STRUCTURAL FIX (ships in alpha.28): (1) `apply-linux.sh` at repo root + in the zip — the Fedora apply is now ONE guarded command: refuses non-Linux, refuses a stale zip (metadata version + adjust/confirm markers checked in a TEMP extract BEFORE the repo is touched), refuses a dirty repo, commits the release-pinned message, pushes, runs install.sh, prints the logout reminder. Tested: stale-zip abort, happy path (correct commit+push), idempotent re-run (nothing to commit), dirty-repo abort. (2) The Windows block is now ONE PowerShell line gated on `$env:OS` — in bash it is a parse error at `{` and ZERO commands execute (verified by feeding it to bash with echo-markers; nothing ran). (3) APPLY.md restructured: "STOP — two machines" table at the very top, one-liner as primary apply, freshness gate says alpha.28, file map 25 files.

### Risk table for round 14 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| Q1: two-step works end-to-end | Super+F8 drag → release → `area preview` journal (nothing stored) → adjust → Confirm/Enter → `area stored` | DONE — v2.3.4 quality pass |
| Q2: handle press eaten by overlay | journal shows redraw (`redrag`) when grabbing a handle, rect jumps | handle actor not picked — check reactive + above-sibling order; move handles to a dedicated top layer |
| Q3: buttons unclickable / click-through | Confirm click does nothing or redraws | St.Button 'clicked' vs modal grab routing; check `_pinSetToolbarActive(true)` ran (adjust entry) |
| Q4: toolbar mispositioned (overlaps rect/offscreen) | bar hidden behind panel or pinned at wrong monitor edge | preferred-width measured before first allocation — re-sync on `notify::allocation` if reported |
| Q5: cursor stuck as resize arrow after Esc | pointer stays NW_RESIZE outside the overlay | `_pinSelectTeardown` resets to DEFAULT; if a crash path skips teardown, Mutter resets on grab end — confirm |
| Everything green | sloppy drags are fixable before storing; Confirm is deliberate | v2.3.4 converged; next = edge-handle elongation, gsettings-persisted per-pin label, Windows F8 UIA analog |

## Task 28 — alpha.28a: Cargo.lock sync (CI fix) + restart-persistence audit

### Field event (round 15 — GitHub Actions `cargo check --locked` exit 101)
- CI on origin failed: `error: cannot update the lock file …/Cargo.lock because --locked was passed to prevent this`. Root cause: **stale lock, not code** — `Cargo.toml` was bumped alpha.24→alpha.28 across five commits while `Cargo.lock` was last touched at alpha.23 (`name = "invertonator" / version = "0.3.0-alpha.23"`). `--locked` refuses to update the lock from the network, so the job died before compiling.
- Verified via `git diff ddd5aa2 HEAD -- Cargo.toml`: only the package version line changed since alpha.23 — zero dependency changes. Complete fix: one-line version sync inside Cargo.lock → `0.3.0-alpha.28` (commit alpha.28a). CI keeps `--locked` (reproducibility); the discipline is "bump Cargo.toml → re-lock → commit both", now noted in APPLY.md's Round 15 section.
- Lesson filed for future bumps: version-string releases must run `cargo check --locked` locally (or touch Cargo.lock) BEFORE packaging the zip — the sandbox has no cargo, so packaging scripts must diff `Cargo.toml` vs `Cargo.lock` versions as a gate.

### Restart-persistence audit (user question: "check if the stored holes survive restart or not")
- **Verdict: YES — entries survive reboot / logout / shell crash; the visible holes are re-baked automatically on the next session.** Code chain (extension.js):
  1. **Disk**: pins live in the `pinned-holes` gsettings key → dconf database (`~/.config/dconf/user`). `_pinConfirm` → `_pinSaveEntries()` writes it the INSTANT Enter/Confirm is pressed, not at logout.
  2. **Load**: the extension constructor runs `_pinLoadEntries()` on every `enable()` — fresh login, re-login, X11 `Alt+F2 r`, extension toggle.
  3. **Apply**: `start-enabled` (auto-written by `setInverted`) drives auto-resume → `setInverted(true)` re-loads entries, runs `_pinsRefresh()` over `global.get_window_actors()` (so PRE-EXISTING matched windows get holes immediately, no focus event needed) and starts the 2s `_pinStartTimer` tick; focus + window-created hooks also refresh.
  4. **Toggle path**: inversion OFF destroys pin effects but keeps gsettings entries (explicit in code); ON reloads and re-applies.
- By-design exceptions (not persistence failures): fullscreen windows drop pins (windowed rel-layout meaningless there — returns on un-fullscreen); minimized windows are skipped until shown; token drift — if a site changes its title shape entirely, a token-matched pin can stop matching (class fallback fires only when the token list is empty). dconf flush timing is near-immediate; a hard power-loss in the same second as Confirm is the only theoretical loss window.
- APPLY.md: added Round 15 CI section + test step 9 (restart persistence: `gsettings get org.gnome.shell.extensions.invertonator pinned-holes` → reboot → auto-resume → `stored area applied` journal ≤2s); Windows block commit message updated to alpha.28a; file map notes the lock sync. No code change — metadata/common.rs stay 0.3.0-alpha.28, zip markers unchanged for `apply-linux.sh`.

## Task 29 — alpha.29: adjust chrome dead-on-click fix (Q3 confirmed, round 16)

### Field report
- USER: "the buttons which show after an area is selected which are confirm, redraw and cancel do not work on clicking" — risk-table Q3, confirmed in the field. Drags/move/redrag kept working (those presses are delivered to the grab actor itself), so the modal grab was alive — only the St.Button `clicked` path was dead.

### Root cause (routing class, not state machine)
- Audited the whole selection state machine (press/motion/release/key, `_pinEnterAdjust` → `_pinSetToolbarActive(true)` at adjust entry, reactive flags) — all correct. The failure sits in DELIVERY: on some Mutter builds the modal grab routes presses to the GRAB actor, so a St.Button never forms its press→release pair and `clicked` never emits. `_pinSelectPress` is bound to the grab actor — the one delivery path proven alive in the field.

### alpha.29 changes (v2.3.5) — two independent click layers, exactly one fires
1. **Press-driven buttons**: Confirm/Redraw/Cancel now fire on `button-press-event` (guaranteed-delivery event), not on `clicked`; `clicked` stays connected as a belt (all three callbacks are idempotent on a torn-down/phase-changed selection, so a stray second call is a no-op).
2. **Overlay hit-test fallback**: `_pinSelectPress` (grab actor, field-proven path) hit-tests the press coordinates against the buttons (`_pinButtonAt`), the toolbar padding (`_pinToolbarAt` — absorb only, prevents a pad click from reading as redraw), and the 8 resize handles (`_pinHandleAt`). Each layer consumes with EVENT_STOP → no double-fire under EITHER Mutter behavior.
3. Handles got the same treatment: `_pinHandlePressXY` (mask/cursor/cursor logic) shared by the actor handler and the fallback — resize now survives grab-starved child delivery too.
4. New state: `s.toolbarActive` gates both layers (replaces reliance on reactive flags alone); `s.btnActions` / `s.handleSpecs` parallel arrays for the hit tests. `_pinSetToolbarActive` sets flag + reactive together.
- Version bump: metadata 29, Cargo/common.rs 0.3.0-alpha.29, Cargo.lock synced in the same commit (the alpha.28a lesson — packaging gate now verifies this), README v2.3.5, apply-linux.sh REPO_V=29 + alpha.29 MSG.

### Risk table for round 16 (pre-registered)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| Q3 resolved: buttons fire | click Confirm → `area stored`; Redraw → dashed box + `redraw requested`; Cancel → overlay closes | DONE if the click test in APPLY.md step 8 passes on Fedora |
| Q3 variant: only SOME clicks dead | hit-test fallback fires from grab actor — journal shows the action on every click | if a click still does nothing, capture journal tail (JS ERROR lines matter) |
| Q2 regression: handle fires from fallback AND actor | double `redraw`/rect jump on single press | EVENT_STOP on both layers prevents it; if seen, the pick is delivering twice — log ev.get_time() |
| Q4/Q5 unchanged | toolbar placement, cursor reset — untouched this round | re-verify in the same pass |

## Task 30 — v0.3.0 STABLE (Linux v2.4.0): desktop-empty auto-off (user request, round 17)

### Field request
- USER: "if there are no windows open and we are on the home page then inversion turns off — add this as a feature, it is not present yet, just an ease of life thing … this will be a stable release, everything perfect, thorough testing, official release." User explicitly designated this cut STABLE → version line promoted: 0.3.0-alpha.29 → **0.3.0** (Cargo/lock/common.rs), metadata 30, Linux v2.4.0.

### Design (deliberate semantics — documented everywhere)
- The global pass inverts EVERY pixel; with no window open it is an inverted wallpaper → while inverted, closing the LAST real window auto-offs (setInverted(false) — identical teardown to a manual toggle; stored pin entries survive, start-enabled cleared, indicator flips).
- Predicate `_desktopWindowCount(exclude)`: whitelist NORMAL/DIALOG/MODAL_DIALOG/UTILITY/SPLASHSCREEN, skip override-redirect — docks, desktop-icon layers, notifications never hold inversion open. Minimized windows count as OPEN (restoring must not need a re-toggle). Windows on other workspaces count (fires on CLOSE, not on workspace visits).
- Event-driven only: the per-window 'unmanaged' hook (every close path ends there) → `_onWindowGone(mw)`. NO polling tick — on purpose: a poll would kill an explicit Super+I on an already-empty desktop seconds later (bounce); pre-arming must work. Login auto-resume behaves the same.
- Escape hatch: new gsettings key `auto-off-no-windows` (default true) in the schema XML + recompiled gschemas.compiled; install.sh schema check extended to SEVEN keys; `_autoOffSetting()` falls open (feature stays on) if a stale schema lacks the key.
- Safety: a Super+F8 selection left open while the last window closes is cancelled before teardown (orphaned grab overlay would freeze input).

### v0.3.0 changes
1. extension.js: header v2.4 block; `_watchWindow` unmanaged handler calls `_onWindowGone(mw)`; new section `_desktopWindowCount` / `_autoOffSetting` / `_onWindowGone` (log + pinSelectCancel(true) + setInverted(false) + Main.notify with accelFor-accelerated hint).
2. Schema: `auto-off-no-windows` key (default true), gschemas.compiled recompiled (glib-compile-schemas present in the pack env; install.sh recompiles on the target anyway), all 8 keys verified present in the compiled binary.
3. metadata.json: version 30 + description sentence. install.sh: key-check loop + feature echo line. apply-linux.sh: REPO_V=30, v0.3.0 MSG, THREE zip content markers (adjust+confirm, auto-off in extension.js, auto-off in schema XML), wording updates.
4. Cargo.toml/Cargo.lock/src/common.rs → 0.3.0 (lock synced in the SAME commit — alpha.28a lesson; packaging gate re-verifies).
5. README (status, platform matrix v2.4.0, history entries v2.3.3–v2.4.0), APPLY.md (title, "What's new in v0.3.0 stable" section, freshness gate `0.3.0`, Fedora/manual commands, SEVEN schema lines, test protocol step 1 Version: 30 + NEW step 10 auto-off tests a–e, Windows block message bumped — it had been left at alpha.28a in round 16, file map refreshed).

### Pre-release verification (in the pack environment, no cargo/GNOME)
- `bash -n` on apply-linux.sh + install.sh; node ESM syntax check on extension.js.
- Mocked-GI scenario harness (scripts/test_extension.mjs): loads the REAL extension.js against stubbed gi:// + resource:// modules and drives the state machine — 15 scenarios, all green (see the run log in the packaging step): two-window close order, last-close auto-off (notify + start-enabled false + effect removed), dock-only desktop fires, override-redirect-only fires, minimized window holds, dialog holds, explicit enable on empty desktop survives, opt-out key holds, setting read failure falls open, not-inverted no-op, selection-cancel path, disable→enable cycle, created-then-closed churn, double-close batch, accelFor notification text.
- Packaging gate: Cargo.toml == Cargo.lock == 0.3.0, metadata 30; every packaged file byte-compared against the workdir.

### Risk table for round 17 (stable release)
| Branch | Evidence in field | Verdict path |
|---|---|---|
| Feature works | close last window → wallpaper back to normal + notification + journal `desktop empty, inversion auto-off` | DONE if APPLY.md step 10 passes |
| Over-firing | inversion drops while a window is still open (dock-only case intended; anything else a bug) | sub-cases a–e isolate which predicate branch miscounts — capture journal tail |
| Under-firing | last close does nothing | check `gsettings get … auto-off-no-windows` first, then journal for `unmanaged`-path JS ERRORs |
| Q3 regression (alpha.29 click fix) | Confirm/Redraw/Cancel clicks | APPLY.md step 8 click test re-run in the same pass |
| Restart persistence | reboot → auto-resume + pin re-apply | APPLY.md step 9 re-run; with an empty desktop at login, inversion resumes and STAYS on (explicit-restore semantics) |
| Schema/keybinding death | hotkeys dead after install | install.sh now verifies SEVEN keys — a missing line means the shipped gschemas.compiled is stale |
