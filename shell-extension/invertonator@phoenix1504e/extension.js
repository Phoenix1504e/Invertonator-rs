// Invertonator — GNOME Shell (Wayland) backend.
//
//   Why an extension? On a Wayland session no client process can touch
//   the display's gamma LUT (XRandR writes via XWayland are discarded by
//   Mutter, and wlroots' gamma protocol is not implemented by GNOME).
//   But the compositor itself composites EVERY output pixel — after
//   every application (and its web content) has already rendered its
//   frame. A shader effect applied to the compositor's stage therefore
//   inverts the whole screen while remaining completely invisible to
//   the web layer — the exact anti-detection property the Windows
//   Magnifier backend and the Linux X11 gamma backend provide:
//   inversion strictly after rendering.
//
//   Mechanism (v1.1): GLSL inversion attached to Main.layoutManager.uiGroup
//   (the tree holding windows, wallpaper and panel — the whole-desktop
//   target proven by murat-cileli/shader-effects on GNOME 48 + 51; the
//   stage actor itself is special-cased by the compositor and its effect
//   path proved unreliable). Cogl runs this over uiGroup's offscreen
//   render:
//
//       cogl_color_out = vec4(1.0 - c.rgb, c.a)
//
//   One fullscreen pass — negligible GPU cost on any 2015+ hardware.
//
//   Hotkey: Super+I (NOT F5 — a shell-level grab of a plain F-key
//   would shadow the browser's refresh shortcut system-wide).
//
//   v2 holes (alpha.20): Super+F7 "pierce" — the Windows Magnifier analog.
//   A hole is the SAME inversion shader attached a SECOND time, to the
//   picked window actor itself. Inversion is an involution (1-x), so a
//   window its own effect paints inverted and uiGroup's global pass
//   paints again comes out TRUE COLOR — a hole punched through the
//   global inversion covering exactly that window (moves, resizes and
//   fullscreening follow automatically; no rectangle tracking, no
//   overlay windows). Super+F7 picks the topmost window under the pointer;
//   press again over the same window to close the hole; turning
//   inversion off clears every hole. (Windows additionally auto-holes
//   media via UIA — the AT-SPI2 auto-media analog is v2.1.)
//
//   v2.1 holes (alpha.22) — "holes everywhere":
//     * Hole manager: the panel menu lists every hole by window title;
//       click a row to close that one hole (per-window bookkeeping made
//       visible). Indicator count + submenu live-update together.
//     * Fullscreen auto-hole (the video feature): while inversion is on,
//       any window that goes fullscreen is pierced AUTOMATICALLY —
//       YouTube fullscreen, mpv, VLC, games come up true color without
//       touching the hotkey (the UIA auto-media analog, heuristic
//       edition; element-level AT-SPI2 rects are v2.2). Manual holes are
//       never auto-removed by un-fullscreening; auto holes are. Opt out:
//       gsettings set org.gnome.shell.extensions.invertonator
//       auto-hole-fullscreen false
//     * Per-tab truth: a browser window is ONE buffer to the compositor
//       — tabs are web content inside it. A hole covers whichever tab
//       is showing and follows tab switches for free; the shell cannot
//       tell tabs apart (that is web content inside one buffer).
//       (alpha.24: element media rects BEAT this hole — see below.)
//     * Hole survival: if Mutter recreates a window actor (remap paths),
//       the hole reattaches to the new actor via an idle re-check;
//       window close drops its hole cleanly (unmanaged safety net).
//     * Direct-scanout guard: inversion disables unredirect/direct
//       scanout where the Mutter API exists, so a fullscreen video can
//       never bypass the compositor shaders.
//     * Pick includes override-redirect windows (context menus,
//       tooltips) — pierce those too.
//
//   v2.2/2.3 media lock-on (alpha.23/24): element-level auto-holes.
//   While inversion is on, the focused window's AT-SPI2 tree is searched
//   for image/video/animation elements and EACH gets its own true-color
//   rect — the rest of the screen stays inverted (the Windows UIA
//   auto-media analog). alpha.24 rework: the hole is ONE shader per
//   window whose GLSL bakes the rects as literal normalized coordinates
//   and inverts only inside them. alpha.23's Clutter.Clone overlay was
//   anchored at stage (0,0) — aligned only for windows already at the
//   origin, shifted copies everywhere else (that is what "auto hole
//   doesn't work" was); clones are gone. Discovery = budgeted cached
//   tree walk + free AT-SPI event capture; element rects BEAT the
//   fullscreen auto-hole (fullscreen browser: video true, chrome
//   inverted) and the whole-window hole stays as the fallback for native
//   players and games. Toolkit accessibility is switched on
//   automatically (GTK3 apps need a restart to pick it up).
//
//   v2.3.3 stored area pins (alpha.27): "select an area once, it gets
//   stored" — Super+F8 opens a drag-select overlay; on release the rect
//   becomes an ADJUSTABLE PREVIEW (v2.3.4: 8 resize handles, drag-inside
//   to move, drag-outside to redraw, Confirm/Redraw/Cancel bar, Enter
//   confirms, R redraws, Esc cancels — NOTHING is stored until Confirm);
//   the confirmed rectangle is stored NORMALIZED to the focused window's
//   frame together with match tokens derived from the window title (a
//   YouTube page always carries a "- youtube" segment; video titles
//   change per video, so tokens match loosely), class match as
//   fallback. Stored areas live in gsettings (pinned-holes) and survive
//   reboots: whenever a window whose title contains a stored token shows
//   up (ANY browser, ANY session), its pin is re-baked as a rect-shader
//   hole at the stored relative position — zero AT-SPI budget, instant,
//   and immune to browsers that hide their a11y tree (LibreWolf hardened builds).
//   The pin rides the window actor like every other rect hole, so moves
//   and resizes follow; a matching window that goes FULLSCREEN drops
//   the pin (a windowed player's position is meaningless fullscreen)
//   and the fullscreen auto-hole / media rects take over. PRIORITY:
//   manual Super+F7 pierce > stored pins > media rects > fullscreen
//   auto-hole.
//
// Tested against the GNOME 45-51 extension API surface (ESM imports,
// Extension.getSettings, Main.wm.addKeybinding, PanelMenu.Button).
//
// IMPORT STYLE — CRITICAL, this bit us in alpha.16: the shell's ui
// modules (main.js, panelMenu.js, popupMenu.js) export NAMED bindings
// only — they have NO default export. The canonical ESM template
// imports them with `import * as Main from ...`. A default import dies
// at module load with:
//   SyntaxError: The requested module 'resource:///org/gnome/shell/ui/main.js'
//   doesn't provide an export named: 'default'
// and the extension never reaches enable() (State: ERROR, no icon, no
// hotkey). gi:// namespaces (Clutter, St, ...) DO have default exports
// and are imported plain.

import Clutter from 'gi://Clutter';
import Cogl from 'gi://Cogl';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const SCHEMA = 'org.gnome.shell.extensions.invertonator';

// v2.3 element media lock-on budgets (mirrors the Windows tracker's
// cadence/evidence design — D-Bus calls cost main-loop time, so every
// number here is a jank guard).
const MEDIA_MAX_HOLES = 6;            // rect cap per window (Windows analog: pool)
const MEDIA_MIN_EDGE = 48;            // icons/thumbnails stay inverted
const MEDIA_WALK_INTERVAL_S = 3;      // cached-walk cadence
const MEDIA_WALK_BUDGET = 480;        // max D-Bus calls per walk (alpha.24:
                                      // 240 starved on real web trees — a
                                      // YouTube page's a11y tree is thousands
                                      // of nodes deep)
const MEDIA_WALK_CHUNK_MS = 8;        // max block per idle chunk
const MEDIA_CACHE_CAP = 1200;         // cached subtree nodes
const MEDIA_REFRESH_MIN_MS = 150;     // extents refresh throttle (scroll)
const MEDIA_WALK_MIN_MS = 1200;       // min gap between full walks
const MEDIA_APP_BLACKLIST_MS = 60000; // hung-app protection
const MEDIA_SLOW_APP_MS = 1200;       // probe slower than this → blacklist
const MEDIA_REBAKE_DELTA_PX = 6;      // re-shader only when a rect moved this far
const MEDIA_REROOT_MIN_MS = 8000;     // alpha.26: min gap between fresh
                                      // revalidation walks (steady state)
const MEDIA_EVENT_PROBES_PER_SEC = 16;// event-source role probes per second cap
const MEDIA_EVENT_HIT_CAP = 24;       // remembered event-captured elements
const MEDIA_EVENT_HIT_TTL_MS = 60000; // stale event captures expire
const MEDIA_FULLSCREEN_SYNC_MS = 1600;// let the first walk claim rects before
                                       // the whole-window fallback fires

// v2.3.3 stored area pins (alpha.27) budgets — pure bookkeeping, no
// D-Bus anywhere: the refresh tick is string matching over window titles.
const PIN_MAX_ENTRIES = 16;           // stored area cap (oldest evicted)
const PIN_MIN_EDGE = 24;              // px — a smaller drag is a misclick
const PIN_MIN_REL = 0.02;             // a smaller stored rect is degenerate
const PIN_TICK_S = 2;                 // re-scan cadence (tab switches)
const PIN_TOKEN_MIN = 4;              // title-token length bounds
const PIN_TOKEN_MAX = 48;
const PIN_TOKEN_CAP = 4;              // tokens stored per pin
const PIN_MAX_STORE = 4;              // per-pin rel precision = 4 decimals
// v2.3.4 two-step selection (alpha.28): after the drag the rect becomes an
// adjustable preview — 8 handles, move-by-drag, Confirm/Redraw/Cancel bar.
const PIN_HANDLE = 13;                // px — adjust-handle square size
const PIN_UI_GAP = 12;                // px — toolbar gap from the rect
const PIN_MARGIN = 8;                 // px — screen-edge clamp margin
// Title segments that name the APP, not the site — never stored as match
// tokens (they would match every window of that app).
const PIN_APP_TOKENS = new Set([
    'mozilla firefox', 'firefox', 'librewolf', 'waterfox', 'tor browser',
    'chromium', 'google chrome', 'chrome', 'brave', 'epiphany', 'web',
    'vivaldi', 'opera', 'microsoft edge', 'edge', 'safari', 'zen browser',
    'mpv', 'vlc', 'celluloid', 'totem', 'gnome mpv', 'haruna', 'dragon player',
]);

// The inversion GLSL — a bare fragment BODY (no main()). Cogl's template
// wraps it; `cogl_sampler0` is AUTO-PROVIDED by Cogl and must never be
// re-declared (duplicate declaration = compile error). Texture access
// incantation proven in murat-cileli/shader-effects on GNOME 48 + 51.
const INVERT_FRAGMENT = `
vec4 c = texture2D(cogl_sampler0, cogl_tex_coord_in[0].xy);
cogl_color_out = vec4(1.0 - c.rgb, c.a);
`;

// Effect machinery — dual API, mirrors murat-cileli/shader-effects
// (whole-desktop GLSL, tested GNOME 48 + 51):
//   GNOME <=49: Shell.GLSLEffect, GLSL injected via vfunc_build_pipeline +
//               add_glsl_snippet(..., is_replace=true)
//   GNOME 50+ : Shell.GLSLEffect REMOVED (gjs.guide "Port to GNOME 51") —
//               Clutter.ShaderEffect takes its GLSL from
//               vfunc_get_static_snippet() -> Cogl.Snippet.set_replace().
//               alpha.18 journal proved set_shader_source() AND the
//               shader-source property are both gone in 51.
const EffectBase = Shell.GLSLEffect ?? Clutter.ShaderEffect;

class InvertonatorEffectImpl extends EffectBase {
}

if (Shell.GLSLEffect) {
    Object.defineProperty(InvertonatorEffectImpl.prototype, 'vfunc_build_pipeline', {
        value() {
            this.add_glsl_snippet(Cogl.SnippetHook.FRAGMENT, '', INVERT_FRAGMENT, true);
        },
    });
} else {
    Object.defineProperty(InvertonatorEffectImpl.prototype, 'vfunc_get_static_snippet', {
        value() {
            const snippet = Cogl.Snippet.new(Cogl.SnippetHook.FRAGMENT, '', null);
            snippet.set_replace(INVERT_FRAGMENT);
            return snippet;
        },
    });
}

const InvertonatorEffect = GObject.registerClass(InvertonatorEffectImpl);

function makeInvertEffect() {
    return new InvertonatorEffect();
}

// v2.3 element-hole fragment: invert ONLY inside the baked rect literals
// (normalized to the actor's paint-volume box). No uniforms — Cogl's GI
// uniform surface is unreliable across GNOME versions; literals are not.
// `cogl_sampler0` / `cogl_tex_coord_in` are Cogl-provided (see above).
function buildRectFragment(rects) {
    let body = 'vec4 c = texture2D(cogl_sampler0, cogl_tex_coord_in[0].xy);\n' +
        'vec2 p = cogl_tex_coord_in[0].xy;\n' +
        'float inside = 0.0;\n';
    for (const r of rects) {
        body += `if (p.x >= ${r[0].toFixed(6)} && p.y >= ${r[1].toFixed(6)} && ` +
            `p.x <= ${r[2].toFixed(6)} && p.y <= ${r[3].toFixed(6)}) inside = 1.0;\n`;
    }
    body += 'cogl_color_out = vec4(mix(c.rgb, vec3(1.0) - c.rgb, inside), c.a);\n';
    return body;
}

class MediaRectEffectImpl extends EffectBase {
}

if (Shell.GLSLEffect) {
    Object.defineProperty(MediaRectEffectImpl.prototype, 'vfunc_build_pipeline', {
        value() {
            this.add_glsl_snippet(Cogl.SnippetHook.FRAGMENT, '', this._frag, true);
        },
    });
} else {
    Object.defineProperty(MediaRectEffectImpl.prototype, 'vfunc_get_static_snippet', {
        value() {
            const snippet = Cogl.Snippet.new(Cogl.SnippetHook.FRAGMENT, '', null);
            snippet.set_replace(this._frag);
            return snippet;
        },
    });
}

const MediaRectEffect = GObject.registerClass(MediaRectEffectImpl);

function makeRectEffect(fragment) {
    const effect = new MediaRectEffect();
    effect._frag = fragment;
    return effect;
}

const Indicator = GObject.registerClass(
    class InvertonatorIndicator extends PanelMenu.Button {
        _init(ext) {
            super._init(0.0, 'Invertonator', false);
            this._ext = ext;

            this._icon = new St.Icon({
                icon_name: 'display-brightness-symbolic',
                style_class: 'system-status-icon',
            });
            this.add_child(this._icon);

            this._item = new PopupMenu.PopupMenuItem('Inversion: OFF');
            this._item.connect('activate', () => this._ext.toggle());
            this.menu.addMenuItem(this._item);

            this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

            const pierceItem = new PopupMenu.PopupMenuItem(
                `Pierce window under pointer (${this._ext.accelFor('toggle-pierce', 'Super+F7')})`
            );
            pierceItem.connect('activate', () => this._ext.pierce());
            this.menu.addMenuItem(pierceItem);

            // v2.3.3: stored area pins — select once, re-applied forever.
            const pinItem = new PopupMenu.PopupMenuItem(
                `Pin a stored area (${this._ext.accelFor('pin-hole', 'Super+F8')})`
            );
            pinItem.connect('activate', () => this._ext.pinSelectStart());
            this.menu.addMenuItem(pinItem);

            // v2.1: per-hole manager — one row per hole, click to close it.
            this._holeSub = new PopupMenu.PopupSubMenuMenuItem('Holes: 0');
            this.menu.addMenuItem(this._holeSub);
            this.rebuildHoles([]);

            const clearItem = new PopupMenu.PopupMenuItem('Clear all holes');
            clearItem.connect('activate', () => this._ext.clearHoles());
            this.menu.addMenuItem(clearItem);

            // v2.3.3: stored-areas manager — one row per pin, click forgets
            // it; the footer forgets everything.
            this._pinSub = new PopupMenu.PopupSubMenuMenuItem('Stored areas: 0');
            this.menu.addMenuItem(this._pinSub);
            this._pinSig = '';
            this.rebuildPins([]);

            this._forgetItem = new PopupMenu.PopupMenuItem('Forget all stored areas');
            this._forgetItem.connect('activate', () => this._ext.pinsForget());
            this.menu.addMenuItem(this._forgetItem);
        }

        setState(inverted, holes = 0) {
            this._item.label.text = inverted
                ? ('Inversion: ON' +
                  (holes > 0 ? ` · ${holes} hole${holes > 1 ? 's' : ''}` : ''))
                : 'Inversion: OFF';
        }

        // v2.1: one row per hole — "title — class (auto)", click closes it.
        rebuildHoles(infos, mediaCount = 0) {
            this._holeSub.menu.removeAll();
            this._holeSub.label.text = `Holes: ${infos.length}`;
            if (infos.length === 0 && mediaCount === 0) {
                this._holeSub.menu.addMenuItem(new PopupMenu.PopupMenuItem(
                    'none — pierce a window first', { reactive: false }));
                return;
            }
            for (const info of infos) {
                const row = new PopupMenu.PopupMenuItem(
                    `${info.title} — ${info.cls}${info.auto ? ' (auto)' : ''}`);
                row.connect('activate', () => this._ext.removeHole(info.mw));
                this._holeSub.menu.addMenuItem(row);
            }
            if (mediaCount > 0) {
                this._holeSub.menu.addMenuItem(new PopupMenu.PopupMenuItem(
                    `media lock-on: ${mediaCount} rect${mediaCount > 1 ? 's' : ''} (auto)`,
                    { reactive: false }));
            }
        }

        // v2.3.3: one row per stored area — "#1 — youtube — 0.03,0.25…",
        // click forgets that pin. Signature-guarded: the refresh tick runs
        // every 2s and must not churn the open menu.
        rebuildPins(entries) {
            const sig = JSON.stringify(entries);
            if (sig === this._pinSig) {
                return;
            }
            this._pinSig = sig;
            this._pinSub.menu.removeAll();
            this._pinSub.label.text = `Stored areas: ${entries.length}`;
            if (entries.length === 0) {
                this._pinSub.menu.addMenuItem(new PopupMenu.PopupMenuItem(
                    'none — Super+F8 stores one', { reactive: false }));
            }
            entries.forEach((e, i) => {
                const match = (e.tokens?.length)
                    ? `title: ${e.tokens.join(', ')}`
                    : `class: ${e.cls || 'any'}`;
                const rel = e.rel.map(v => Math.round(v * 100) / 100).join(', ');
                const row = new PopupMenu.PopupMenuItem(
                    `#${i + 1} — ${match} — [${rel}]`);
                row.connect('activate', () => this._ext.pinDelete(i));
                this._pinSub.menu.addMenuItem(row);
            });
            if (this._forgetItem) {
                this._forgetItem.reactive = entries.length > 0;
                this._forgetItem.label.text = entries.length > 0
                    ? `Forget all stored areas (${entries.length})`
                    : 'Forget all stored areas';
            }
        }
    }
);

export default class InvertonatorExtension extends Extension {
    enable() {
        this._settings = this.getSettings(SCHEMA);
        this._inverted = false;
        // Meta.Window -> {mw, actor, effect, destroyId, unmanagedId, auto}
        this._holes = new Map();
        // Meta.Window -> {notifyId, unmanagedId} — fullscreen auto-hole
        this._fullscreenWatch = new Map();
        this._unredirectDisabled = false;
        this._windowCreatedId = 0;
        // v2.3 element media lock-on — walker + per-window rect-hole state.
        // holes: Map Meta.Window -> {mw, actor, effect, rects[], nRects[],
        //   destroyId, unmanagedId, sizeId} — ONE shader per window holding
        //   up to MEDIA_MAX_HOLES baked rects.
        this._media = {
            atspi: null, roleMap: null, atspiFailed: false, initStarted: false,
            running: false, timerId: 0, walkTimerId: 0, walkScheduled: false,
            chunkId: 0, eventPubId: 0, focusTimerId: 0, walkActive: false,
            walkComplete: false, callBudget: 0,
            queue: [], seen: null, found: [], targetMw: null,
            apps: null, appsTs: 0, slowApps: new Map(), cache: new Map(),
            eventHits: new Map(), probeTs: 0, resizeTs: 0,
            holes: new Map(), lastWalk: 0, lastRefresh: 0,
            publishedCount: -1, hinted: false, eventCbs: [],
            diagWalkTs: 0, diagLastFound: -1, diag: new Set(),
            // alpha.25 walker-convergence state: the BFS queue CARRIES OVER
            // between ticks (progress every tick even when the cache never
            // hits), cache keys probe for a stable identity, tree-shape
            // events force a re-root.
            cacheKeyMode: null, cacheHits: 0, carried: 0, reRoot: false,
            // alpha.26: steady-state revalidation gate + document-subtree
            // priority set (see _mediaWalk / _mediaWalkChunk).
            lastRootTs: 0, docRoles: null,
        };
        this._focusId = 0;
        // v2.3.3 stored area pins — entries persist in gsettings, effects
        // are per-window rect shaders (same machinery as the media rects).
        this._pins = new Map();      // Meta.Window -> pin hole object
        this._pinEntries = [];       // stored pins (loaded from settings)
        this._pinTimerId = 0;
        this._pinSelect = null;      // selection-mode state
        this._pinGrab = null;
        this._pinLoadEntries();

        try {
            this._effect = makeInvertEffect();
            console.log('Invertonator: effect ready (base=' +
                (Shell.GLSLEffect ? 'Shell.GLSLEffect' : 'Clutter.ShaderEffect') + ')');
        } catch (e) {
            this._effect = null;
            console.error(
                `Invertonator: shader effect could not be created (${e}) — ` +
                'inversion will be unavailable; check journalctl for details'
            );
        }

        this._indicator = new Indicator(this);
        Main.panel.addToStatusArea('invertonator', this._indicator);

        this._addBinding('toggle-invert', () => this.toggle());
        this._addBinding('toggle-pierce', () => this.pierce());
        this._addBinding('pin-hole', () => this.pinSelectStart());

        // v2.1: fullscreen auto-hole — watch every window's fullscreen
        // state (existing windows now, future windows via window-created).
        this._windowCreatedId = global.display.connect('window-created',
            (display, mw) => {
                this._watchWindow(mw);
                this._pinsRefresh();
            });
        this._watchAllWindows();

        // v2.2: element media lock-on — focus changes retarget the walker;
        // v2.3.3: pins re-scan immediately on focus (cheap: string ops).
        this._focusId = global.display.connect('notify::focus-window', () => {
            this._mediaFocusChanged();
            this._pinsRefresh();
        });
        this._media.initStarted = true;
        this._mediaInitAtspi();

        if (this._settings.get_boolean('start-enabled')) {
            this.setInverted(true);
        }
        console.log(`Invertonator ${this.metadata.version} enabled — ` +
            'Super+I invert · Super+F7 pierce · Super+F8 pin area · ' +
            'fullscreen auto-hole · media lock-on');
    }

    disable() {
        // Colors must come back before we vanish — never leave a user
        // staring at an inverted screen with the toggle gone. Also clears
        // every hole (they are meaningless without the global pass).
        this.setInverted(false);
        if (this._windowCreatedId) {
            try { global.display.disconnect(this._windowCreatedId); } catch (e) {}
            this._windowCreatedId = 0;
        }
        this._clearFullscreenWatch();
        this._setUnredirectDisabled(false);
        this._mediaShutdown();
        // v2.3.3: selection mode must never survive the extension (a
        // leaked grab would freeze input), then effects, then the timer.
        this.pinSelectCancel(true);
        this._pinStopTimer();
        this._pinsDestroyAll();
        if (this._focusId) {
            try { global.display.disconnect(this._focusId); } catch (e) {}
            this._focusId = 0;
        }
        try {
            Main.wm.removeKeybinding('toggle-invert');
        } catch (e) {}
        try {
            Main.wm.removeKeybinding('toggle-pierce');
        } catch (e) {}
        try {
            Main.wm.removeKeybinding('pin-hole');
        } catch (e) {}
        this._indicator.destroy();
        this._indicator = null;
        this._effect = null;
        this._holes = null;
        this._fullscreenWatch = null;
        this._unredirectDisabled = false;
        this._media = null;
        this._pins = null;
        this._pinEntries = [];
        this._settings = null;
    }

    toggle() {
        this.setInverted(!this._inverted);
    }

    _addBinding(name, handler) {
        try {
            const action = Main.wm.addKeybinding(
                name,
                this._settings,
                Meta.KeyBindingFlags.NONE,
                Shell.ActionMode.ALL,
                handler
            );
            // alpha.21: registration now journals itself — a silent miss
            // (alpha.20's F7) cost a whole field round to find.
            console.log(`Invertonator: keybinding '${name}' registered ` +
                `(action=${action}, accel=${JSON.stringify(this._settings.get_strv(name))})`);
        } catch (e) {
            console.error(`Invertonator: keybinding '${name}' failed (${e}) — ` +
                'the panel menu still works');
        }
    }

    // --- Linux v2: true-color holes (F7 pierce) -------------------------
    //
    // A hole is the SAME inversion shader attached a second time to the
    // picked window's actor. Inversion is an involution (1 - x), so a
    // window painted inverted by its own effect and painted again by
    // uiGroup's global pass comes out TRUE COLOR. The hole IS the window:
    // moves, resizes and fullscreening follow automatically — no
    // rectangle tracking, no overlay windows. (Clutter applies a
    // parent's effects to the already-painted subtree, so uiGroup's pass
    // sees post-hole pixels — that ordering is the whole trick.)

    // F7 analog of the Windows pierce: toggle a hole on the topmost
    // window under the pointer.
    pierce() {
        if (!this._inverted) {
            console.log('Invertonator: pierce ignored — inversion is off');
            Main.notify('Invertonator',
                'Turn inversion on first (Super+I), then press F7 over a window.');
            return;
        }

        const [px, py] = global.get_pointer();
        const candidates = [];
        for (const actor of global.get_window_actors()) {
            const mw = actor.get_meta_window?.();
            if (!mw || mw.minimized) {
                continue;
            }
            // v2.1: override-redirect windows (menus, tooltips) skip the
            // workspace check — they belong to no workspace at all.
            const isOr = mw.is_override_redirect?.() ?? false;
            if (!isOr && !mw.showing_on_its_workspace()) {
                continue;
            }
            const r = mw.get_frame_rect();
            if (px >= r.x && px < r.x + r.width &&
                py >= r.y && py < r.y + r.height) {
                candidates.push({ mw, actor });
            }
        }
        if (candidates.length === 0) {
            console.log('Invertonator: pierce found no window under the pointer');
            Main.notify('Invertonator', 'No window under the pointer.');
            return;
        }

        // Stacking is bottom-first — the last hit is the topmost window
        // under the pointer. sort_windows_by_stacking() is the
        // authoritative sort where available; the array order is the
        // fallback. (The two only disagree on exact overlaps.)
        let top = candidates[candidates.length - 1];
        try {
            const sorted = global.display.sort_windows_by_stacking(
                candidates.map(c => c.mw)
            );
            const lastMw = sorted[sorted.length - 1];
            const idx = candidates.findIndex(c => c.mw === lastMw);
            if (idx >= 0) {
                top = candidates[idx];
            }
        } catch (e) {}

        const cls = top.mw.get_wm_class() ?? 'unknown';
        const title = top.mw.get_title() ?? 'window';
        if (this._holes.has(top.mw)) {
            this.removeHole(top.mw);
            console.log(`Invertonator: hole OFF (${cls}) "${title}"`);
            Main.notify('Invertonator', `Hole OFF: ${title}`);
        } else {
            this._addHole(top.actor, top.mw);
            console.log(`Invertonator: hole ON (${cls}) "${title}" — ` +
                'window re-inverts the global pass = true color');
            Main.notify('Invertonator', `Hole ON: ${title}`);
        }
        this._refreshHoleUI();
    }

    clearHoles() {
        if (!this._holes || this._holes.size === 0) {
            return;
        }
        const n = this._holes.size;
        for (const mw of [...this._holes.keys()]) {
            this.removeHole(mw);
        }
        console.log(`Invertonator: cleared ${n} hole(s)`);
        this._refreshHoleUI();
    }

    _addHole(actor, mw, auto = false) {
        const hole = {
            mw, actor, effect: makeInvertEffect(),
            destroyId: 0, unmanagedId: 0, auto,
        };
        hole.destroyId = actor.connect('destroy', () =>
            this._onHoleActorDestroyed(mw, hole));
        hole.unmanagedId = mw.connect('unmanaged', () => {
            // Window closed — the actor destroy usually fires first and
            // handles the drop; this is the safety net.
            if (this._holes && this._holes.get(mw) === hole) {
                this._holes.delete(mw);
                this._refreshHoleUI();
                console.log('Invertonator: hole dropped (unmanaged): ' +
                    (mw.get_title?.() ?? '?'));
            }
        });
        actor.add_effect(hole.effect);
        this._holes.set(mw, hole);
        // A window-hole already renders this window true color — element
        // rects on top would re-invert its content. Drop them.
        this._mediaDropForWindow(mw);
        // v2.3.3: same for stored pins — the whole-window hole wins while
        // it lives; removing the hole restores the pin (_pinsRefresh).
        this._pinDestroyFor(mw);
        this._refreshHoleUI();
        return hole;
    }

    removeHole(mw) {
        const hole = this._holes?.get(mw);
        if (!hole) {
            return;
        }
        try {
            hole.actor.remove_effect(hole.effect);
            if (hole.destroyId) {
                hole.actor.disconnect(hole.destroyId);
            }
            if (hole.unmanagedId) {
                mw.disconnect(hole.unmanagedId);
            }
        } catch (e) {}
        this._holes.delete(mw);
        // v2.3.3: a stored pin for this window may now come back.
        this._pinsRefresh();
        this._refreshHoleUI();
    }

    _onHoleActorDestroyed(mw, hole) {
        if (!this._holes || this._holes.get(mw) !== hole) {
            return;
        }
        // Mutter can recreate a window actor on remap paths (monitor
        // changes, some reconfigurations). Give the new actor a beat to
        // appear, then reattach; if the window is really gone, drop the
        // hole. The dying actor takes the old effect with it — nothing to
        // remove, only new bookkeeping.
        GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
            if (!this._holes || this._holes.get(mw) !== hole) {
                return GLib.SOURCE_REMOVE;
            }
            let next = null;
            try {
                next = mw.get_compositor_private?.() ?? null;
            } catch (e) {}
            if (next && next !== hole.actor) {
                console.log('Invertonator: hole reattached to a recreated actor (' +
                    (mw.get_title?.() ?? '?') + ')');
                hole.actor = next;
                hole.effect = makeInvertEffect();
                hole.destroyId = next.connect('destroy', () =>
                    this._onHoleActorDestroyed(mw, hole));
                next.add_effect(hole.effect);
            } else {
                this._holes.delete(mw);
                try {
                    if (hole.unmanagedId) {
                        mw.disconnect(hole.unmanagedId);
                    }
                } catch (e) {}
                console.log('Invertonator: hole dropped (window closed): ' +
                    (mw.get_title?.() ?? '?'));
                this._refreshHoleUI();
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    listHoles() {
        const infos = [];
        for (const [mw, hole] of this._holes) {
            infos.push({
                mw,
                title: mw.get_title?.() ?? 'unnamed window',
                cls: mw.get_wm_class?.() ?? 'unknown',
                auto: hole.auto,
            });
        }
        return infos;
    }

    _refreshHoleUI() {
        this._indicator?.setState(
            this._inverted, this._holes ? this._holes.size : 0);
        let mediaRects = 0;
        if (this._media) {
            for (const hole of this._media.holes.values()) {
                mediaRects += hole.rects.length;
            }
        }
        this._indicator?.rebuildHoles(
            this._holes ? this.listHoles() : [], mediaRects);
        // v2.3.3: stored-areas submenu (signature-guarded inside).
        this._indicator?.rebuildPins(this._pinEntries ?? []);
    }

    // --- v2.1: fullscreen auto-hole (the video feature) -----------------
    //
    // The Windows backend auto-holes media via UIA classification. The
    // Wayland analog starts with the fullscreen heuristic: a window that
    // goes fullscreen while inversion is on is pierced automatically —
    // YouTube fullscreen, mpv, VLC, games come up true color without
    // touching the hotkey. Manual (auto=false) holes are NEVER
    // auto-removed by un-fullscreening; auto holes are. Element-level
    // holes (video element only, chrome stays dark) need AT-SPI2 rects —
    // v2.2.

    _watchAllWindows() {
        for (const actor of global.get_window_actors()) {
            const mw = actor.get_meta_window?.();
            if (mw) {
                this._watchWindow(mw);
            }
        }
    }

    _watchWindow(mw) {
        if (!this._fullscreenWatch || this._fullscreenWatch.has(mw)) {
            return;
        }
        try {
            this._fullscreenWatch.set(mw, {
                notifyId: mw.connect('notify::fullscreen', () =>
                    this._onFullscreenChange(mw)),
                unmanagedId: mw.connect('unmanaged', () => {
                    this._fullscreenWatch?.delete(mw);
                }),
            });
            // The handler is state-driven — this call covers windows that
            // are ALREADY fullscreen at watch time (launched fullscreen
            // while inverted).
            this._onFullscreenChange(mw);
        } catch (e) {
            console.warn(`Invertonator: fullscreen watch failed (${e})`);
        }
    }

    _onFullscreenChange(mw) {
        if (!this._holes || !this._inverted) {
            return;
        }
        let isFs = false;
        try {
            isFs = mw.fullscreen ?? mw.is_fullscreen?.() ?? false;
        } catch (e) {}
        let actor = null;
        try {
            actor = mw.get_compositor_private?.() ?? null;
        } catch (e) {}
        if (!actor) {
            return;
        }
        const setting = !this._settings ||
            this._settings.get_boolean('auto-hole-fullscreen');
        // v2.3.3: a stored pin describes the WINDOWED layout — fullscreen
        // has a different geometry entirely. Drop the pin and let the
        // fullscreen auto-hole / media rects take over; un-fullscreening
        // restores it via _pinsRefresh below.
        if (isFs && this._pins?.has(mw)) {
            this._pinDestroyFor(mw);
            console.log('Invertonator: stored area dropped (window went ' +
                'fullscreen) "' + (mw.get_title?.() ?? '?') + '"');
        }
        // v2.3: element rects beat the whole-window hole — a fullscreen
        // window whose media rects are already locked on keeps its chrome
        // inverted; the whole-window hole is only the fallback.
        if (isFs && setting && !this._holes.has(mw) &&
            !(this._media?.holes?.has(mw))) {
            const title = mw.get_title?.() ?? 'window';
            this._addHole(actor, mw, true);
            console.log(`Invertonator: auto-hole ON (fullscreen) "${title}"`);
            Main.notify('Invertonator', `Auto-hole ON: ${title}`);
        } else if (!isFs) {
            const hole = this._holes.get(mw);
            if (hole && hole.auto) {
                console.log('Invertonator: auto-hole OFF (left fullscreen) "' +
                    (mw.get_title?.() ?? '?') + '"');
                this.removeHole(mw);
            }
            // Un-fullscreened: the stored pin may return.
            this._pinsRefresh();
        }
    }

    _syncFullscreenHoles() {
        // Inversion just turned ON — hole every currently-fullscreen
        // watched window (subject to the setting).
        if (!this._fullscreenWatch) {
            return;
        }
        for (const mw of [...this._fullscreenWatch.keys()]) {
            this._onFullscreenChange(mw);
        }
    }

    _clearFullscreenWatch() {
        if (!this._fullscreenWatch) {
            return;
        }
        for (const [mw, entry] of this._fullscreenWatch) {
            try { mw.disconnect(entry.notifyId); } catch (e) {}
            try { mw.disconnect(entry.unmanagedId); } catch (e) {}
        }
        this._fullscreenWatch.clear();
    }

    // --- v2.1: direct-scanout guard --------------------------------------
    //
    // A fullscreen video window may be direct-scanned out, skipping the
    // compositor FBO entirely — our shaders would never see its pixels.
    // The uiGroup offscreen effect already forces compositing, so this is
    // belt-and-braces insurance; the API moved across Mutter versions, so
    // try both names.

    _setUnredirectDisabled(dis) {
        if (this._unredirectDisabled === dis) {
            return;
        }
        try {
            if (global.compositor?.disable_unredirect) {
                if (dis) {
                    global.compositor.disable_unredirect();
                } else {
                    global.compositor.enable_unredirect();
                }
                console.log(`Invertonator: unredirect ${dis ? 'disabled' : 'enabled'} (compositor API)`);
            } else if (Meta.disable_unredirect_for_display) {
                if (dis) {
                    Meta.disable_unredirect_for_display(global.display);
                } else {
                    Meta.enable_unredirect_for_display(global.display);
                }
                console.log(`Invertonator: unredirect ${dis ? 'disabled' : 'enabled'} (Meta API)`);
            } else {
                console.log('Invertonator: no unredirect API — uiGroup effect already forces compositing');
            }
            this._unredirectDisabled = dis;
        } catch (e) {
            console.warn(`Invertonator: unredirect toggle failed (${e})`);
        }
    }

    // --- v2.3: element media lock-on (AT-SPI2 rect holes) ----------------
    //
    // The Windows backend walks the foreground UIA tree and punches holes
    // at media elements. The Wayland analog, alpha.24 edition:
    //
    //   RENDERING — ONE Clutter.ShaderEffect per target window whose GLSL
    //   bakes the media rects as literal normalized coordinates: inside a
    //   rect the window's pixels are inverted a second time, uiGroup's
    //   pass re-inverts them (involution) → true color; everything else
    //   stays once-inverted. The effect RIDES the window actor, so moving
    //   the window moves its holes for free — no overlays, no clones, no
    //   position tracking. Rects are normalized against the actor's
    //   paint-volume box: exactly the bounds Cogl's offscreen texture
    //   spans for this effect, so shadows/CSD margins cancel by
    //   construction. (alpha.23's Clutter.Clone overlay translated the
    //   cloned content to stage (0,0) — aligned only for windows AT the
    //   origin, shifted window copies everywhere else. That is what "auto
    //   hole doesn't work" was.)
    //
    //   DISCOVERY — budgeted, chunked BFS over the FOCUSED window's
    //   AT-SPI2 tree (image/video/animation roles), PLUS free event-side
    //   capture: AT-SPI events carry their source element, so media that
    //   announces itself (bounds/showing storms when a player starts, on
    //   page loads) is captured with ZERO walk budget and matched to a
    //   window by rect containment — even when pid/name resolution fails.
    //   A slow app is blacklisted for a minute; a budget-starved walk
    //   never wipes holes (it only adds the event captures).
    //
    //   PRIORITY — element rects BEAT the fullscreen auto-hole: a
    //   fullscreen browser with a detected video gets the video true and
    //   chrome still inverted (the v2.2 request); windows whose tree
    //   proves NO media on a complete walk keep the whole-window
    //   fullscreen hole as the fallback (native players, games). Manual
    //   Super+F7 holes beat everything.
    //
    //   EVIDENCE (Windows alpha.11/14 analogs): extents are always live
    //   queries; walks are budgeted/chunked so D-Bus never janks the
    //   shell; every swap, fallback and API miss journals itself.

    async _mediaInitAtspi() {
        const m = this._media;
        if (!m || m.atspi || m.atspiFailed) {
            return;
        }
        try {
            // DYNAMIC import — a failing static import would kill the whole
            // module (alpha.16 lesson); this way Atspi problems only cost
            // the lock-on feature, never inversion.
            const mod = await import('gi://Atspi');
            const Atspi = mod.default ?? mod;
            const roleMap = new Map();
            for (const [name, kind] of [['IMAGE', 'image'], ['VIDEO', 'video'],
                ['ANIMATION', 'animation']]) {
                if (Atspi.Role?.[name] !== undefined) {
                    roleMap.set(Atspi.Role[name], kind);
                }
            }
            if (roleMap.size === 0) {
                m.atspiFailed = true;
                console.warn('Invertonator: AT-SPI2 has no IMAGE/VIDEO roles — lock-on disabled');
                return;
            }
            m.atspi = Atspi;
            m.roleMap = roleMap;
            // alpha.26: document-subtree roles get BFS priority — once the
            // walk reaches a document node (content or chrome), its subtree
            // is explored BEFORE the rest of the queue. Without this the
            // walk drowned in browser chrome / sidebar siblings and only
            // reached the content document after thousands of calls.
            const docRoles = new Set();
            for (const name of ['DOCUMENT', 'DOCUMENT_WEB', 'DOCUMENT_FRAME',
                'DOCUMENT_EMAIL', 'DOCUMENT_PRESENTATION',
                'DOCUMENT_SPREADSHEET', 'DOCUMENT_TEXT']) {
                if (Atspi.Role?.[name] !== undefined) {
                    docRoles.add(Atspi.Role[name]);
                }
            }
            m.docRoles = docRoles;
            console.log(`Invertonator: AT-SPI2 ready — media lock-on roles: ` +
                [...roleMap.values()].join(','));
            this._mediaConnectEvents();
            this._mediaStart();
        } catch (e) {
            m.atspiFailed = true;
            console.warn(`Invertonator: element lock-on unavailable (Atspi import: ${e}) — ` +
                'manual + fullscreen holes unaffected');
        }
    }

    _mediaConnectEvents() {
        const m = this._media;
        const types = ['object:bounds-changed', 'object:state-changed:showing',
            'document:load-complete', 'document:load-stopped',
            'object:children-changed'];
        let ok = 0;
        let apiName = '';
        for (const t of types) {
            try {
                // Callback shape is binding-dependent ((listener,event) vs
                // (event)) — pick the argument that carries a source.
                const cb = (...args) => {
                    const ev = args.find(a => a && a.source !== undefined) ?? null;
                    this._mediaOnEvent(t, ev);
                };
                let listener = null;
                try {
                    m.atspi.EventListener.add_callback(cb, t);
                    apiName = 'EventListener.add_callback';
                } catch (e1) {
                    // alpha.25: GI surfaces without the static callback
                    // helper — instance listener + register() instead.
                    listener = m.atspi.EventListener.new(cb);
                    listener.register(t);
                    apiName = 'EventListener.register';
                }
                m.eventCbs.push({ cb, t, listener });
                ok++;
            } catch (e) {
                console.warn(`Invertonator: AT-SPI event '${t}' unavailable (${e})`);
            }
        }
        // alpha.25: the capture path journals itself — a silent no-op here
        // costs a whole field round (the F7 lesson).
        if (ok === 0) {
            console.warn('Invertonator: event capture OFF (no listener API) — ' +
                'element holes rely on the periodic walk only');
        } else {
            console.log(`Invertonator: event capture on via ${apiName} ` +
                `(${ok}/${types.length} listeners)`);
        }
    }

    _mediaOnEvent(type, ev) {
        const m = this._media;
        if (!m?.running || !this._inverted) {
            return;
        }
        const now = Date.now();
        // Free discovery: event sources are probed (rate-limited) for media
        // roles — a player announcing itself costs no walk budget.
        if (ev?.source &&
            now - m.probeTs >= Math.floor(1000 / MEDIA_EVENT_PROBES_PER_SEC)) {
            m.probeTs = now;
            try {
                const kind = m.roleMap.get(ev.source.get_role());
                if (kind) {
                    m.eventHits.set(ev.source, { kind, ts: now });
                    if (m.eventHits.size > MEDIA_EVENT_HIT_CAP) {
                        let oldest = null;
                        let oldestTs = Infinity;
                        for (const [acc, hit] of m.eventHits) {
                            if (hit.ts < oldestTs) {
                                oldestTs = hit.ts;
                                oldest = acc;
                            }
                        }
                        if (oldest) {
                            m.eventHits.delete(oldest);
                        }
                    }
                    this._mediaScheduleEventPublish();
                }
            } catch (e) {}
        }
        // Tree-shape events → full (budgeted, cached) walk; move events →
        // cheap extents-only refresh (the scroll-follow path).
        if (type.startsWith('document:')) {
            // alpha.26: a page LOAD is a genuinely new tree — re-root.
            m.reRoot = true;
            if (now - m.lastWalk > MEDIA_WALK_MIN_MS) {
                this._mediaScheduleWalk(60);
            }
        } else if (type === 'object:children-changed') {
            // alpha.26 FIELD FIX (round 11): children-changed NO LONGER
            // re-roots. YouTube emits it CONTINUOUSLY while playing
            // (progress bar, buffering, DOM churn) — re-rooting on every
            // event restarted the walk inside the browser chrome forever,
            // never reaching the content document (journal: media=0 across
            // six 480-call walks, carried=0 after the first). The carried
            // queue keeps converging; this event only accelerates the
            // schedule, and the steady-state guard in _mediaWalk does the
            // periodic revalidation.
            if (now - m.lastWalk > MEDIA_WALK_MIN_MS) {
                this._mediaScheduleWalk(60);
            }
        } else if (now - m.lastRefresh >= MEDIA_REFRESH_MIN_MS) {
            m.lastRefresh = now;
            this._mediaRefreshExtents();
        }
    }

    _mediaFocusChanged() {
        const m = this._media;
        if (!m?.running) {
            return;
        }
        if (m.focusTimerId) {
            GLib.source_remove(m.focusTimerId);
        }
        m.focusTimerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 400, () => {
            m.focusTimerId = 0;
            this._mediaScheduleWalk(0);
            return GLib.SOURCE_REMOVE;
        });
    }

    _mediaStart() {
        const m = this._media;
        if (!m || m.running || !m.atspi || m.atspiFailed) {
            return;
        }
        if (!this._settings?.get_boolean('auto-hole-media')) {
            return;
        }
        m.running = true;
        m.timerId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT,
            MEDIA_WALK_INTERVAL_S, () => this._mediaTick());
        this._mediaHintA11y();
        this._mediaScheduleWalk(150);
    }

    _mediaStop() {
        const m = this._media;
        if (!m || !m.running) {
            return;
        }
        m.running = false;
        for (const id of [m.timerId, m.walkTimerId, m.focusTimerId, m.chunkId,
            m.eventPubId]) {
            if (id) {
                try { GLib.source_remove(id); } catch (e) {}
            }
        }
        m.timerId = m.walkTimerId = m.focusTimerId = m.chunkId = m.eventPubId = 0;
        m.walkScheduled = false;
        m.walkActive = false;
        m.walkComplete = false;
        m.queue = [];
        m.seen = null;
        m.targetMw = null;
        m.reRoot = false;
        m.eventHits.clear();
        this._mediaClearHoles();
    }

    _mediaShutdown() {
        const m = this._media;
        if (!m) {
            return;
        }
        this._mediaStop();
        for (const { cb, t, listener } of m.eventCbs) {
            // alpha.25: deregistration now covers both registration APIs —
            // the static helper's counterpart and instance listener.
            try {
                m.atspi?.EventListener?.remove_callback?.(cb, t);
            } catch (e) {}
            try {
                m.atspi?.EventListener?.deregister_callback?.(cb, t);
            } catch (e) {}
            try {
                listener?.deregister?.(t);
            } catch (e) {}
        }
        m.eventCbs = [];
        m.cache.clear();
        m.eventHits.clear();
        m.diag.clear();
        m.apps = null;
    }

    _mediaTick() {
        const m = this._media;
        if (!m?.running) {
            return GLib.SOURCE_REMOVE;
        }
        if (!this._inverted) {
            return GLib.SOURCE_CONTINUE;
        }
        if (!this._settings?.get_boolean('auto-hole-media')) {
            this._mediaStop();
            return GLib.SOURCE_REMOVE;
        }
        this._mediaScheduleWalk(0);
        return GLib.SOURCE_CONTINUE;
    }

    _mediaHintA11y() {
        const m = this._media;
        if (m.hinted) {
            return;
        }
        m.hinted = true;
        try {
            const iface = Gio.Settings.new('org.gnome.desktop.interface');
            if (!iface.get_boolean('toolkit-accessibility')) {
                // Set, not hint: GTK3 apps only expose their a11y tree when
                // this is on. Already-running GTK3 apps need a restart to
                // pick it up; GTK4/Firefox/Chromium usually react to the
                // a11y bus alone.
                iface.set_boolean('toolkit-accessibility', true);
                console.log('Invertonator: toolkit-accessibility was OFF — set ON ' +
                    '(already-running GTK3 apps need a restart to expose their trees)');
            }
        } catch (e) {
            console.warn(`Invertonator: toolkit-accessibility check failed (${e})`);
        }
    }

    _mediaScheduleWalk(delayMs) {
        const m = this._media;
        if (!m?.running || m.walkScheduled || m.walkActive) {
            return;
        }
        m.walkScheduled = true;
        m.walkTimerId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delayMs, () => {
            m.walkTimerId = 0;
            m.walkScheduled = false;
            this._mediaWalk();
            return GLib.SOURCE_REMOVE;
        });
    }

    _mediaScheduleEventPublish() {
        const m = this._media;
        if (!m?.running || m.eventPubId) {
            return;
        }
        m.eventPubId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 700, () => {
            m.eventPubId = 0;
            if (m.running && this._inverted && !m.walkActive) {
                this._mediaRefreshExtents();
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    _mediaApiMiss(name, e) {
        const m = this._media;
        if (!m.apiMissed) {
            m.apiMissed = true;
            console.warn(`Invertonator: AT-SPI API miss '${name}' (${e}) — ` +
                'element lock-on degraded; report the journal line');
        }
    }

    _mediaDiagOnce(key, line) {
        const m = this._media;
        if (!m || m.diag.has(key)) {
            return;
        }
        m.diag.add(key);
        console.log(line);
    }

    _mediaWalk() {
        const m = this._media;
        if (!m?.running || !this._inverted || m.walkActive) {
            return;
        }
        const now = Date.now();
        if (now - m.lastWalk < MEDIA_WALK_MIN_MS) {
            return;
        }
        m.lastWalk = now;
        m.walkComplete = false;
        const mw = global.display.get_focus_window?.();
        if (!mw || (mw.get_pid?.() ?? 0) <= 0 || mw.minimized) {
            // Nothing focused: element holes cannot belong anywhere — a
            // COMPLETE empty publish drops them and lets the fullscreen
            // fallback re-arm on fullscreen windows.
            m.queue = [];
            m.seen = null;
            m.targetMw = null;
            m.reRoot = false;
            this._mediaPublish(null, true);
            return;
        }
        m.walkActive = true;
        m.callBudget = MEDIA_WALK_BUDGET;
        m.cacheHits = 0;
        // alpha.26: a reRoot (page load) discards any carried queue — it is
        // a genuinely new tree.
        if (m.reRoot) {
            m.queue = [];
            m.seen = null;
            m.targetMw = null;
            m.reRoot = false;
        }
        // alpha.25 CONVERGENCE: same target window → keep walking the
        // carried-over queue instead of restarting from the roots. A
        // budget-starved walk therefore makes REAL progress every tick
        // (480 fresh nodes per tick) and eventually drains.
        const carrying = mw === m.targetMw && m.queue.length > 0;
        if (carrying) {
            m.carried = m.queue.length;
            this._mediaWalkChunk();
            return;
        }
        // alpha.26 STEADY-STATE GUARD (field round 11): queue drained, same
        // window, no page load → do NOT re-root every tick. A cheap extents
        // refresh keeps the existing holes glued; a fresh revalidation walk
        // re-roots at most once per MEDIA_REROOT_MIN_MS (new DOM content is
        // picked up there — with cache hits it is cheap, without them it is
        // still bounded instead of thrashing 480 calls per second-cadence).
        if (mw === m.targetMw &&
            Date.now() - m.lastRootTs < MEDIA_REROOT_MIN_MS) {
            m.walkActive = false;
            if (now - m.lastRefresh >= MEDIA_REFRESH_MIN_MS) {
                m.lastRefresh = now;
                this._mediaRefreshExtents();
            }
            return;
        }
        m.carried = 0;
        m.found = [];
        m.targetMw = mw;
        m.lastRootTs = Date.now();
        this._mediaResolveFrames(mw, (roots) => {
            if (!m.walkActive) {
                return;
            }
            if (!roots || roots.length === 0) {
                m.walkActive = false;
                // Resolution failed — event captures still count as proof;
                // a clean "nothing" lets the fullscreen fallback re-arm.
                this._mediaPublish(null, true);
                return;
            }
            m.queue = roots;
            m.seen = new Set(roots.map(r => r.acc));
            this._mediaWalkChunk();
        });
    }

    // Map the focused Meta.Window to its AT-SPI2 application (by PID, with
    // wm_class name as fallback), return its top-level frames pre-calibrated:
    // cal = {k, ex, ey} — screen(p) = frameRect.xy + (p - frameExt.xy) * k.
    // The k-calibration self-corrects ANY coordinate-space discrepancy
    // (HiDPI scaling, toolkit quirks) because it is measured, not assumed.
    _mediaResolveFrames(mw, done) {
        const m = this._media;
        const Atspi = m.atspi;
        const pid = mw.get_pid();
        const wmClass = (mw.get_wm_class?.() ?? '').toLowerCase();
        const now = Date.now();

        if (!m.apps || now - m.appsTs > 10000) {
            m.apps = [];
            m.appsTs = now;
            m.cache.clear();   // stale wrappers from the previous snapshot
            let desktop = null;
            try {
                desktop = Atspi.get_desktop(0);
            } catch (e) {
                this._mediaApiMiss('get_desktop', e);
                done(null);
                return;
            }
            let n = 0;
            const t0 = Date.now();
            try {
                n = desktop.get_child_count();
            } catch (e) {
                this._mediaApiMiss('child_count', e);
                done(null);
                return;
            }
            if (Date.now() - t0 > MEDIA_SLOW_APP_MS) {
                // The a11y bus itself is sick — back off entirely.
                m.slowApps.set(-1, now);
            }
            n = Math.min(n, 25);
            for (let i = 0; i < n; i++) {
                let app = null;
                try {
                    app = desktop.get_child_at_index(i);
                } catch (e) {}
                if (!app) {
                    continue;
                }
                let appPid = -1;
                let name = '';
                try {
                    appPid = app.get_process_id?.() ?? -1;
                    name = (app.get_name?.() ?? '').toLowerCase();
                } catch (e) {}
                m.apps.push({ acc: app, pid: appPid, name });
            }
        }

        if (m.slowApps.has(-1) && now - m.slowApps.get(-1) < MEDIA_APP_BLACKLIST_MS) {
            done(null);
            return;
        }

        let app = m.apps.find(a => a.pid === pid);
        if (!app && wmClass) {
            app = m.apps.find(a =>
                a.name.includes(wmClass) || wmClass.includes(a.name));
        }
        if (!app) {
            this._mediaDiagOnce(`noapp:${pid}:${wmClass}`,
                `Invertonator: no AT-SPI app for pid=${pid} class="${wmClass}" — ` +
                'event capture still applies; if holes never appear, check the ' +
                'toolkit-accessibility journal line');
            done(null);
            return;
        }
        if (m.slowApps.has(app.pid) &&
            now - m.slowApps.get(app.pid) < MEDIA_APP_BLACKLIST_MS) {
            done(null);
            return;
        }

        const coord = Atspi.CoordType?.SCREEN ?? 0;
        const fr = mw.get_frame_rect();
        const roots = [];
        let cnt = 0;
        const t1 = Date.now();
        try {
            cnt = app.acc.get_child_count();
            m.callBudget--;
        } catch (e) {
            this._mediaApiMiss('app child_count', e);
            done(null);
            return;
        }
        if (Date.now() - t1 > MEDIA_SLOW_APP_MS) {
            m.slowApps.set(app.pid, now);
            done(null);
            return;
        }
        cnt = Math.min(cnt, 12);
        for (let i = 0; i < cnt && m.callBudget > 0; i++) {
            let frame = null;
            try {
                frame = app.acc.get_child_at_index(i);
            } catch (e) {}
            m.callBudget--;
            if (!frame) {
                continue;
            }
            let ext = null;
            try {
                const comp = frame.getComponent?.();
                ext = comp?.get_extents?.(coord);
            } catch (e) {}
            m.callBudget--;
            // k = measured atspi→logical scale (1.0 when spaces agree).
            let k = 1;
            let ex = 0, ey = 0;
            if (ext && ext.width > 0) {
                const kGuess = fr.width / ext.width;
                if (kGuess > 0.2 && kGuess < 5) {
                    k = kGuess;
                }
                ex = ext.x;
                ey = ext.y;
            }
            roots.push({ acc: frame, cal: { k, ex, ey }, frameAcc: frame });
        }
        // alpha.25: probe the cache-key identity ONCE on a real node. A
        // stable object path beats wrapper identity (wrappers are not
        // guaranteed to survive D-Bus re-fetches); when unavailable, the
        // queue carry-over still guarantees the walk converges.
        if (m.cacheKeyMode === null && roots.length > 0) {
            let p = null;
            try {
                p = roots[0].acc.get_path?.();
            } catch (e) {}
            m.cacheKeyMode = (typeof p === 'string' && p.length > 0)
                ? 'path' : 'wrap';
            console.log(`Invertonator: a11y cache keying: ${m.cacheKeyMode}` +
                (m.cacheKeyMode === 'path'
                    ? ' (stable object paths — cache hits across walks)'
                    : ' (wrapper identity — carry-over ensures convergence)'));
        }
        done(roots);
    }

    // Budgeted, chunked BFS. Cached nodes cost ZERO D-Bus calls (role +
    // children reused from the previous walk — the Windows "cached walk"
    // analog); only new nodes and live media extents pay. alpha.25: the
    // queue carries over between ticks (see _mediaWalk), so a starved walk
    // converges over consecutive ticks instead of restarting.
    _mediaWalkChunk() {
        const m = this._media;
        if (!m?.running || !m.walkActive) {
            return;
        }
        const Atspi = m.atspi;
        const coord = Atspi.CoordType?.SCREEN ?? 0;
        const t0 = Date.now();
        while (m.queue.length > 0 && m.callBudget > 0 &&
               Date.now() - t0 < MEDIA_WALK_CHUNK_MS) {
            const { acc, cal, frameAcc } = m.queue.shift();
            m.seen.add(acc);
            const cached = m.cache.get(this._mediaCacheKey(acc));
            let role;
            if (cached) {
                m.cacheHits++;
                role = cached.role;
            } else {
                try {
                    role = acc.get_role();
                } catch (e) {
                    continue;
                }
                m.callBudget--;
            }
            const kind = m.roleMap.get(role);
            if (kind) {
                // MEDIA — extents are ALWAYS live (they move with layout).
                let ext = null;
                try {
                    const comp = acc.getComponent?.();
                    ext = comp?.get_extents?.(coord);
                } catch (e) {}
                m.callBudget--;
                if (ext && ext.width >= MEDIA_MIN_EDGE &&
                    ext.height >= MEDIA_MIN_EDGE) {
                    m.found.push({ acc, kind, ext, cal, frameAcc });
                }
            } else {
                let kids = cached?.kids;
                if (!kids) {
                    kids = [];
                    let cnt = 0;
                    try {
                        cnt = acc.get_child_count();
                        m.callBudget--;
                    } catch (e) {
                        continue;   // dead or hung node — nothing to cache
                    }
                    const cap = Math.min(cnt, 80);
                    for (let i = 0; i < cap && m.callBudget > 0; i++) {
                        try {
                            const k = acc.get_child_at_index(i);
                            if (k) {
                                kids.push(k);
                            }
                            m.callBudget--;
                        } catch (e) {}
                    }
                    if (m.cache.size > MEDIA_CACHE_CAP) {
                        m.cache.clear();
                    }
                    m.cache.set(this._mediaCacheKey(acc),
                        { role, kids, ts: Date.now() });
                }
                // alpha.26 DOCUMENT PRIORITY: a document node's subtree is
                // explored before everything else (sibling order kept on
                // both paths — reversed iteration + unshift/push).
                const dive = m.docRoles?.has(role) ?? false;
                for (let i = kids.length - 1; i >= 0; i--) {
                    const k = kids[i];
                    if (m.seen.has(k)) {
                        continue;
                    }
                    if (dive) {
                        m.queue.unshift({ acc: k, cal, frameAcc });
                    } else {
                        m.queue.push({ acc: k, cal, frameAcc });
                    }
                }
            }
        }
        // A single hung app can stall each AT-SPI call for the bus timeout —
        // if this chunk overran badly, blacklist the app and abort the walk.
        if (Date.now() - t0 > MEDIA_SLOW_APP_MS) {
            const pid = m.targetMw?.get_pid?.() ?? -1;
            m.slowApps.set(pid, Date.now());
            m.queue = [];
            m.seen = null;
            m.targetMw = null;
            m.walkActive = false;
            this._mediaPublish(null, false);   // keep holes; event captures only
            return;
        }
        if (m.queue.length > 0 && m.callBudget > 0) {
            m.chunkId = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
                m.chunkId = 0;
                this._mediaWalkChunk();
                return GLib.SOURCE_REMOVE;
            });
        } else {
            m.walkActive = false;
            m.walkComplete = m.queue.length === 0;
            // alpha.25: a tree-shape event arrived mid-walk — the carried
            // queue is stale; re-root on the next walk.
            if (m.reRoot) {
                m.queue = [];
                m.seen = null;
                m.targetMw = null;
                m.reRoot = false;
            }
            if (m.walkComplete) {
                const entries = this._mediaEntriesFromFound();
                this._mediaWalkDiag(entries.length);
                this._mediaPublish(entries, true);
            } else {
                // Budget starved mid-tree: publish event captures only and
                // keep every existing hole — a wipe here would thrash the
                // fullscreen fallback on huge trees. The queue CARRIES OVER
                // (alpha.25) so the next tick continues deeper into the tree.
                // Journal the convergence so silent starvation is visible.
                const now = Date.now();
                if (now - m.diagWalkTs > 8000) {
                    m.diagWalkTs = now;
                    const app = m.apps?.find(
                        a => a.pid === (m.targetMw?.get_pid?.() ?? -1));
                    console.log(`Invertonator: a11y walk converging ` +
                        `"${app?.name ?? '?'}" — ` +
                        `queue=${m.queue.length} carried=${m.carried} ` +
                        `media=${m.found.length} ` +
                        `calls=${MEDIA_WALK_BUDGET - m.callBudget} ` +
                        `hits=${m.cacheHits} key=${m.cacheKeyMode ?? '?'}`);
                }
                this._mediaPublish(null, false);
            }
        }
    }

    // alpha.25: cache identity. GI wrappers are NOT guaranteed to be stable
    // across D-Bus re-fetches; the object path is. The mode is probed once
    // on a real node and journaled — when only wrapper keying is possible
    // the queue carry-over still guarantees convergence.
    _mediaCacheKey(acc) {
        const m = this._media;
        if (m.cacheKeyMode !== 'path') {
            return acc;
        }
        try {
            const p = acc.get_path?.();
            if (typeof p === 'string' && p.length > 0) {
                return p;
            }
        } catch (e) {}
        return acc;
    }

    _mediaWalkDiag(found) {
        const m = this._media;
        const now = Date.now();
        if (found === m.diagLastFound && now - m.diagWalkTs < 30000) {
            return;
        }
        m.diagLastFound = found;
        m.diagWalkTs = now;
        const app = m.apps?.find(
            a => a.pid === (m.targetMw?.get_pid?.() ?? -1));
        console.log(`Invertonator: a11y walk "${app?.name ?? '?'}" ` +
            `calls=${MEDIA_WALK_BUDGET - m.callBudget} media=${found} ` +
            `cache=${m.cache.size} hits=${m.cacheHits} ` +
            `carried=${m.carried ?? 0} key=${m.cacheKeyMode ?? '?'} ` +
            `complete=${m.walkComplete}`);
    }

    _mediaEntriesFromFound() {
        const m = this._media;
        const mw = m.targetMw;
        if (!mw || m.found.length === 0) {
            return [];
        }
        // Sort by area, cap the pool.
        const sorted = [...m.found].sort((a, b) =>
            (b.ext.width * b.ext.height) - (a.ext.width * a.ext.height))
            .slice(0, MEDIA_MAX_HOLES);
        const fr = mw.get_frame_rect();
        const entries = [];
        for (const f of sorted) {
            // cal.ex/ey were measured at walk start; the Δ between frame
            // and element cancels any window movement in between.
            const x = fr.x + (f.ext.x - f.cal.ex) * f.cal.k;
            const y = fr.y + (f.ext.y - f.cal.ey) * f.cal.k;
            const w = f.ext.width * f.cal.k;
            const h = f.ext.height * f.cal.k;
            const e = this._mediaClipEntry(mw, x, y, w, h, f.kind);
            if (e) {
                e.acc = f.acc;
                e.frameAcc = f.frameAcc;
                e.cal = f.cal;
                entries.push(e);
            }
        }
        return entries;
    }

    // Clip a screen-space rect to the window frame; keep it only when the
    // window really covers it (>=50% intersection) — rects leaking from
    // another window of the same app must not punch holes.
    _mediaClipEntry(mw, x, y, w, h, kind) {
        let fr = null;
        try {
            fr = mw.get_frame_rect();
        } catch (e) {
            return null;
        }
        const cx = Math.max(x, fr.x);
        const cy = Math.max(y, fr.y);
        const cw = Math.min(x + w, fr.x + fr.width) - cx;
        const ch = Math.min(y + h, fr.y + fr.height) - cy;
        if (cw < MEDIA_MIN_EDGE || ch < MEDIA_MIN_EDGE) {
            return null;
        }
        if (cw * ch * 2 < w * h) {
            return null;
        }
        return { mw, kind, x: cx, y: cy, w: cw, h: ch };
    }

    // Event-captured media: extents live-queried, matched to whichever
    // VISIBLE window contains the rect center — this path works even when
    // app↔window resolution (pid/name) fails entirely.
    _mediaEventEntries() {
        const m = this._media;
        if (m.eventHits.size === 0) {
            return [];
        }
        const now = Date.now();
        const wins = [];
        for (const actor of global.get_window_actors()) {
            const mw = actor.get_meta_window?.();
            if (!mw || mw.minimized) {
                continue;
            }
            const isOr = mw.is_override_redirect?.() ?? false;
            if (!isOr && !mw.showing_on_its_workspace?.()) {
                continue;
            }
            wins.push(mw);
        }
        const coord = m.atspi.CoordType?.SCREEN ?? 0;
        const entries = [];
        for (const [acc, hit] of m.eventHits) {
            if (now - hit.ts > MEDIA_EVENT_HIT_TTL_MS) {
                m.eventHits.delete(acc);
                continue;
            }
            let ext = null;
            try {
                const comp = acc.getComponent?.();
                ext = comp?.get_extents?.(coord);
            } catch (e) {
                m.eventHits.delete(acc);   // element died
                continue;
            }
            if (!ext || ext.width < MEDIA_MIN_EDGE ||
                ext.height < MEDIA_MIN_EDGE) {
                continue;
            }
            let mw = null;
            for (const w of wins) {
                const fr = w.get_frame_rect();
                const px = ext.x + ext.width / 2;
                const py = ext.y + ext.height / 2;
                if (px >= fr.x && px < fr.x + fr.width &&
                    py >= fr.y && py < fr.y + fr.height) {
                    mw = w;
                    break;
                }
            }
            if (!mw) {
                continue;
            }
            const e = this._mediaClipEntry(mw, ext.x, ext.y, ext.width,
                ext.height, hit.kind);
            if (e) {
                e.acc = acc;
                entries.push(e);
            }
        }
        return entries;
    }

    // Diff-and-rebuild the per-window rect-hole pool. A window's shader is
    // re-baked only when a rect moved >= MEDIA_REBAKE_DELTA_PX or the rect
    // count changed — scrolling keeps the same holes and just moves them.
    //
    // complete=true  → authoritative: the pool is REPLACED and the
    //                  fullscreen fallback may re-arm. Only complete walks
    //                  (and clean empty walks) publish complete=true.
    // complete=false → supplemental: update/create from fresh entries, but
    //                  never wipe holes that this publish has no data for
    //                  (event captures and starved walks are partial views).
    _mediaPublish(walkEntries, complete) {
        const m = this._media;
        if (!m || !m.running) {
            return;
        }
        // Element rects never fight a manual pierce hole; v2.3.3: a stored
        // pin is user intent too — media rects never fight it either.
        const skip = (mw) => this._holes?.has(mw) || this._pins?.has(mw);
        const byWin = new Map();
        const addEntry = (e) => {
            if (!e || skip(e.mw)) {
                return;
            }
            if (!byWin.has(e.mw)) {
                byWin.set(e.mw, []);
            }
            byWin.get(e.mw).push(e);
        };
        for (const e of walkEntries ?? []) {
            addEntry(e);
        }
        for (const e of this._mediaEventEntries()) {
            addEntry(e);
        }
        const rectSets = new Map();
        for (const [mw, list] of byWin) {
            rectSets.set(mw, list.sort((a, b) => (b.w * b.h) - (a.w * a.h))
                .slice(0, MEDIA_MAX_HOLES));
        }

        // PRIORITY: element rects beat the fullscreen auto-hole — the
        // whole-window hole is dropped the moment rects are proven by a
        // COMPLETE walk (starved walks never flip-flop).
        if (complete && this._inverted) {
            for (const [mw, rects] of rectSets) {
                const hole = this._holes?.get(mw);
                if (hole && hole.auto) {
                    console.log(`Invertonator: media rects (${rects.length}) beat ` +
                        `fullscreen hole — chrome re-inverts on ` +
                        `"${mw.get_title?.() ?? '?'}"`);
                    this.removeHole(mw);
                }
            }
        }

        let totalRects = 0;
        for (const [mw, rects] of rectSets) {
            totalRects += rects.length;
            const cur = m.holes.get(mw);
            if (cur && this._rectSetSame(cur.rects, rects)) {
                continue;
            }
            if (cur) {
                this._rectHoleRebuild(mw, rects);
            } else {
                this._rectHoleCreate(mw, rects);
            }
        }
        if (complete) {
            for (const mw of [...m.holes.keys()]) {
                if (!rectSets.has(mw)) {
                    this._rectHoleDestroy(mw);
                }
            }
        }

        // FALLBACK: a fullscreen window whose tree proves NO media on a
        // complete walk (native player, game) gets the whole-window hole.
        if (complete && this._inverted && m.running &&
            (this._settings?.get_boolean('auto-hole-fullscreen') ?? true)) {
            for (const mw of [...(this._fullscreenWatch?.keys() ?? [])]) {
                if (rectSets.has(mw) || this._holes?.has(mw) ||
                    this._pins?.has(mw)) {
                    continue;
                }
                let isFs = false;
                try {
                    isFs = mw.fullscreen ?? mw.is_fullscreen?.() ?? false;
                } catch (e) {}
                if (!isFs) {
                    continue;
                }
                const actor = mw.get_compositor_private?.();
                if (actor && !this._holes?.has(mw)) {
                    this._addHole(actor, mw, true);
                    console.log(`Invertonator: auto-hole ON (fullscreen, no media ` +
                        `nodes) "${mw.get_title?.() ?? '?'}"`);
                }
            }
        }

        const countChanged = totalRects !== m.publishedCount;
        m.publishedCount = totalRects;
        if (countChanged) {
            const kinds = [...new Set(
                [...rectSets.values()].flat().map(r => r.kind))].join(',');
            console.log(`Invertonator: media lock-on ${totalRects} rect(s) on ` +
                `${rectSets.size} window(s)` + (totalRects ? ` (${kinds})` : ''));
            this._refreshHoleUI();
        }
    }

    _rectSetSame(oldRects, newRects) {
        if (oldRects.length !== newRects.length) {
            return false;
        }
        for (let i = 0; i < oldRects.length; i++) {
            const a = oldRects[i];
            const b = newRects[i];
            if (a.kind !== b.kind ||
                Math.abs(a.x - b.x) >= MEDIA_REBAKE_DELTA_PX ||
                Math.abs(a.y - b.y) >= MEDIA_REBAKE_DELTA_PX ||
                Math.abs(a.w - b.w) >= MEDIA_REBAKE_DELTA_PX ||
                Math.abs(a.h - b.h) >= MEDIA_REBAKE_DELTA_PX) {
                return false;
            }
        }
        return true;
    }

    // THE element hole: one shader on the window actor whose GLSL bakes
    // the rects (normalized to the actor's paint-volume box — the same
    // bounds Cogl's offscreen texture spans) and inverts ONLY inside
    // them. uiGroup's pass re-inverts those pixels → true color media,
    // chrome stays inverted. The effect rides the actor: moving the
    // window moves its holes for free.
    _rectHoleCreate(mw, rects) {
        const m = this._media;
        if (!m) {
            return;
        }
        try {
            const actor = mw.get_compositor_private?.();
            if (!actor) {
                return;
            }
            const baked = this._mediaBakeRects(actor, rects, mw);
            if (!baked || baked.length === 0) {
                return;
            }
            const hole = {
                mw, actor,
                effect: makeRectEffect(buildRectFragment(baked)),
                rects: rects.map(r => ({ ...r })),
                nRects: baked,
                destroyId: 0, unmanagedId: 0, sizeId: 0,
            };
            hole.destroyId = actor.connect('destroy', () => {
                if (m.holes.get(mw) === hole) {
                    m.holes.delete(mw);
                    this._refreshHoleUI();
                }
            });
            hole.unmanagedId = mw.connect('unmanaged', () => {
                if (m.holes.get(mw) === hole) {
                    m.holes.delete(mw);
                    this._refreshHoleUI();
                }
            });
            hole.sizeId = mw.connect('size-changed', () =>
                this._mediaOnWindowResized(mw));
            actor.add_effect(hole.effect);
            actor.queue_repaint?.();
            m.holes.set(mw, hole);
        } catch (err) {
            console.warn(`Invertonator: element hole create failed (${err})`);
        }
    }

    _rectHoleRebuild(mw, rects) {
        const m = this._media;
        const cur = m?.holes.get(mw);
        if (!cur) {
            return;
        }
        try {
            const baked = this._mediaBakeRects(cur.actor, rects, mw);
            if (!baked || baked.length === 0) {
                this._rectHoleDestroy(mw);
                return;
            }
            try {
                cur.actor.remove_effect(cur.effect);
            } catch (e) {}
            cur.effect = makeRectEffect(buildRectFragment(baked));
            cur.rects = rects.map(r => ({ ...r }));
            cur.nRects = baked;
            cur.actor.add_effect(cur.effect);
            cur.actor.queue_repaint?.();
        } catch (err) {
            console.warn(`Invertonator: element hole rebuild failed (${err})`);
        }
    }

    _rectHoleDestroy(mw) {
        const m = this._media;
        const cur = m?.holes.get(mw);
        if (!cur) {
            return;
        }
        m.holes.delete(mw);
        try { cur.actor.disconnect(cur.destroyId); } catch (e) {}
        try { cur.mw.disconnect(cur.unmanagedId); } catch (e) {}
        try { cur.mw.disconnect(cur.sizeId); } catch (e) {}
        try { cur.actor.remove_effect(cur.effect); } catch (e) {}
        try { cur.actor.queue_repaint?.(); } catch (e) {}
        this._refreshHoleUI();
    }

    _mediaClearHoles() {
        const m = this._media;
        if (!m) {
            return;
        }
        for (const mw of [...m.holes.keys()]) {
            this._rectHoleDestroy(mw);
        }
        m.publishedCount = 0;
    }

    _mediaDropForWindow(mw) {
        const m = this._media;
        if (!m || !m.holes.has(mw)) {
            return;
        }
        this._rectHoleDestroy(mw);
    }

    _mediaOnWindowResized(mw) {
        const m = this._media;
        if (!m?.running || !this._inverted) {
            return;
        }
        const now = Date.now();
        if (now - m.resizeTs < MEDIA_REFRESH_MIN_MS) {
            return;
        }
        m.resizeTs = now;
        this._mediaRefreshExtents();
    }

    // Screen-space rects → normalized [x0, y0, x1, y1] against the actor's
    // paint-volume box — the bounds Cogl's offscreen texture spans for this
    // actor's effect. Both sides live in the actor's local space, so CSD
    // shadows/margins cancel by construction.
    _mediaBakeRects(actor, rects, mw) {
        let ax = 0, ay = 0;
        try {
            [ax, ay] = actor.get_transformed_position();
        } catch (e) {}
        const box = this._mediaLocalBox(actor);
        if (box.w <= 0 || box.h <= 0) {
            this._mediaDiagOnce('degenerate-box',
                'Invertonator: degenerate paint box — element rects skipped');
            return null;
        }
        const cls = mw.get_wm_class?.() ?? '?';
        this._mediaDiagOnce(`bake:${cls}`,
            `Invertonator: rect bake "${cls}" actor@(${ax},${ay}) ` +
            `box=(${box.x},${box.y},${box.w},${box.h})`);
        const baked = [];
        for (const r of rects) {
            const nx0 = Math.min(Math.max((r.x - ax - box.x) / box.w, 0), 1);
            const ny0 = Math.min(Math.max((r.y - ay - box.y) / box.h, 0), 1);
            const nx1 = Math.min(Math.max((r.x + r.w - ax - box.x) / box.w, 0), 1);
            const ny1 = Math.min(Math.max((r.y + r.h - ay - box.y) / box.h, 0), 1);
            if (nx1 - nx0 < 0.004 || ny1 - ny0 < 0.004) {
                continue;
            }
            baked.push([nx0, ny0, nx1, ny1]);
        }
        if (rects.length > 0 && baked.length === 0) {
            // alpha.25: every rect fell outside the paint box — that is the
            // paint-volume-mismatch signature (worklog risk R1), not a
            // discovery failure. Journal it once per window class.
            this._mediaDiagOnce(`bake-clip:${mw.get_wm_class?.() ?? '?'}`,
                `Invertonator: ALL ${rects.length} element rect(s) clipped ` +
                `during bake (box=${box.x},${box.y},${box.w},${box.h} ` +
                `actor@${ax},${ay}) — paint-volume mismatch suspected; ` +
                'report this journal line');
        }
        return baked;
    }

    // Bounds of the region the effect's offscreen texture covers, in the
    // actor's local space: paint volume first (authoritative — the
    // offscreen pass measures the same volume), paint box as fallback,
    // allocation last.
    _mediaLocalBox(actor) {
        try {
            const pv = actor.get_paint_volume?.();
            if (pv) {
                const o = pv.get_origin?.();
                const w = pv.get_width?.();
                const h = pv.get_height?.();
                if (o && w > 1 && h > 1) {
                    return { x: o.x, y: o.y, w, h };
                }
            }
        } catch (e) {}
        try {
            const [ok, pb] = actor.get_paint_box?.() ?? [false, null];
            if (ok && pb && pb.width > 1 && pb.height > 1) {
                return { x: pb.x, y: pb.y, w: pb.width, h: pb.height };
            }
        } catch (e) {}
        return { x: 0, y: 0, w: actor.width ?? 0, h: actor.height ?? 0 };
    }

    // Scroll-follow: live extents re-query for EXISTING holes only
    // (≤ 6 elements × 2 calls per window, no tree walk), merged with the
    // event captures. Publishes with complete=false — existing holes move
    // or are updated, nothing is wiped or invented here.
    _mediaRefreshExtents() {
        const m = this._media;
        if (!m?.running || m.walkActive) {
            return;
        }
        const coord = m.atspi.CoordType?.SCREEN ?? 0;
        const entries = [];
        for (const [mw, hole] of m.holes) {
            let fr = null;
            try {
                fr = mw.get_frame_rect();
            } catch (e) {
                continue;
            }
            for (const r of hole.rects) {
                if (!r.acc) {
                    continue;
                }
                try {
                    const comp = r.acc.getComponent?.();
                    const ext = comp?.get_extents?.(coord);
                    if (!ext || ext.width <= 0) {
                        continue;
                    }
                    let x, y, w, h;
                    if (r.cal) {
                        // Walk-sourced rect: re-anchor through the frame's
                        // live extents (Δ between element and frame cancels
                        // any window movement in between).
                        let k = r.cal.k;
                        let ex = r.cal.ex;
                        let ey = r.cal.ey;
                        const fcomp = r.frameAcc?.getComponent?.();
                        const fext = fcomp?.get_extents?.(coord);
                        if (fext && fext.width > 0) {
                            const kNow = fr.width / fext.width;
                            if (kNow > 0.2 && kNow < 5) {
                                k = kNow;
                            }
                            ex = fext.x;
                            ey = fext.y;
                        }
                        x = fr.x + (ext.x - ex) * k;
                        y = fr.y + (ext.y - ey) * k;
                        w = ext.width * k;
                        h = ext.height * k;
                    } else {
                        // Event-sourced rect: the extent IS screen space.
                        x = ext.x;
                        y = ext.y;
                        w = ext.width;
                        h = ext.height;
                    }
                    const e = this._mediaClipEntry(mw, x, y, w, h, r.kind);
                    if (e) {
                        e.acc = r.acc;
                        e.frameAcc = r.frameAcc ?? null;
                        e.cal = r.cal ?? null;
                        entries.push(e);
                    }
                } catch (e) {}
            }
        }
        for (const e of this._mediaEventEntries()) {
            entries.push(e);
        }
        this._mediaPublish(entries, false);
    }

    // --- v2.3.3: stored area pins (select once, re-applied forever) ------
    //
    // The user's insight: some layouts NEVER change (YouTube's player is
    // always in the same place relative to the window), so a manually
    // selected area can be STORED and re-applied forever — no tree walk,
    // no a11y bus, works on browsers that hide their accessibility tree.
    //
    //   STORAGE — the rect is stored normalized to the anchor window's
    //   frame (rel 0..1), plus match tokens derived from the title: the
    //   title is split on its dash/pipe separators and stable segments are
    //   kept ("Song X - YouTube — Mozilla Firefox" → tokens ["song x",
    //   "youtube"], the browser suffix is dropped). Video titles change;
    //   a token list matches ANY window whose title contains ANY token —
    //   the next video still matches via "- youtube". No stable segments
    //   → fall back to matching the window class. Pins live in gsettings
    //   (pinned-holes) and survive reboots.
    //
    //   APPLY — a cheap 2s scan (string ops only) + focus/created hooks
    //   re-bake one rect-shader hole per matching window at the stored
    //   relative position. The hole rides the window actor: moves are
    //   free, resizes re-bake via size-changed. Matching windows that go
    //   fullscreen drop their pin (windowed layout is meaningless there).
    //
    //   PRIORITY — pins beat media rects (user intent > discovery) and
    //   lose to manual Super+F7 whole-window holes while those live.

    _pinLoadEntries() {
        this._pinEntries = [];
        try {
            const raw = this._settings.get_string('pinned-holes');
            const parsed = JSON.parse(raw || '[]');
            if (Array.isArray(parsed)) {
                this._pinEntries = parsed.filter(e => e &&
                    Array.isArray(e.rel) && e.rel.length === 4 &&
                    e.rel.every(v => typeof v === 'number' && isFinite(v)) &&
                    typeof e.cls === 'string' &&
                    Array.isArray(e.tokens) &&
                    e.tokens.every(t => typeof t === 'string'));
            }
        } catch (e) {
            console.warn(`Invertonator: stored areas unreadable (${e}) — reset`);
            this._pinSaveEntries();
        }
    }

    _pinSaveEntries() {
        try {
            this._settings.set_string('pinned-holes',
                JSON.stringify(this._pinEntries));
        } catch (e) {
            console.warn(`Invertonator: storing areas failed (${e})`);
        }
        this._refreshHoleUI();
    }

    // Title → match tokens: split on dash/pipe separators, drop app-name
    // segments, keep short stable-looking ones.
    _pinTokensFromTitle(title) {
        const raw = (title ?? '').toLowerCase();
        if (!raw) {
            return [];
        }
        const tokens = [];
        for (let seg of raw.split(/\s*(?:—|–|\|| - )\s*/)) {
            seg = (seg ?? '').trim();
            if (seg.length < PIN_TOKEN_MIN || seg.length > PIN_TOKEN_MAX) {
                continue;
            }
            if (PIN_APP_TOKENS.has(seg)) {
                continue;
            }
            if (tokens.includes(seg)) {
                continue;
            }
            tokens.push(seg);
            if (tokens.length >= PIN_TOKEN_CAP) {
                break;
            }
        }
        return tokens;
    }

    _pinMatches(mw, entry) {
        if (!entry) {
            return false;
        }
        const tokens = entry.tokens ?? [];
        if (tokens.length > 0) {
            // Title-first: ANY stored token in the window title matches.
            let title = '';
            try {
                title = (mw.get_title() ?? '').toLowerCase();
            } catch (e) {}
            return tokens.some(t => title.includes(t));
        }
        const cls = (mw.get_wm_class?.() ?? '').toLowerCase();
        const stored = (entry.cls ?? '').toLowerCase();
        if (stored) {
            return cls.includes(stored) || stored.includes(cls);
        }
        return false;
    }

    _pinEntryFor(mw) {
        if (!this._pinEntries?.length) {
            return null;
        }
        for (let i = this._pinEntries.length - 1; i >= 0; i--) {
            if (this._pinMatches(mw, this._pinEntries[i])) {
                return this._pinEntries[i];
            }
        }
        return null;
    }

    // The scan: ensure every matching VISIBLE window carries its pin
    // effect and no non-matching window does. Cheap on purpose — called
    // every 2s, on focus changes and window creation.
    _pinsRefresh() {
        if (!this._pins || !this._inverted) {
            return;
        }
        let changed = 0;
        const seen = new Set();
        for (const actor of global.get_window_actors()) {
            const mw = actor.get_meta_window?.();
            if (!mw || mw.minimized) {
                continue;
            }
            if (mw.is_override_redirect?.() ?? false) {
                continue;
            }
            if (!mw.showing_on_its_workspace?.()) {
                continue;
            }
            let isFs = false;
            try {
                isFs = mw.fullscreen ?? mw.is_fullscreen?.() ?? false;
            } catch (e) {}
            if (isFs) {
                continue;   // fullscreen layout ≠ windowed layout
            }
            seen.add(mw);
            if (this._holes.has(mw)) {
                continue;   // whole-window hole wins; skip silently
            }
            const entry = this._pinEntryFor(mw);
            const cur = this._pins.get(mw);
            if (cur && cur.entry === entry) {
                continue;
            }
            if (cur) {
                this._pinDestroyFor(mw);
                changed++;
            }
            if (entry && this._pinCreate(mw, entry)) {
                changed++;
            }
        }
        for (const mw of [...this._pins.keys()]) {
            if (!seen.has(mw)) {
                this._pinDestroyFor(mw);
                changed++;
            }
        }
        if (changed > 0) {
            this._refreshHoleUI();
        }
    }

    // Bake the stored rel rect onto the window actor. Returns true when
    // the pin effect actually landed.
    _pinCreate(mw, entry) {
        try {
            const actor = mw.get_compositor_private?.();
            if (!actor) {
                return false;
            }
            const fr = mw.get_frame_rect();
            if (!fr || fr.width <= 0 || fr.height <= 0) {
                return false;
            }
            const rel = entry.rel;
            const x = fr.x + rel[0] * fr.width;
            const y = fr.y + rel[1] * fr.height;
            const w = Math.max((rel[2] - rel[0]) * fr.width, 1);
            const h = Math.max((rel[3] - rel[1]) * fr.height, 1);
            const e = this._mediaClipEntry(mw, x, y, w, h, 'pin');
            if (!e) {
                return false;
            }
            const baked = this._mediaBakeRects(actor, [e], mw);
            if (!baked || baked.length === 0) {
                return false;
            }
            const pin = {
                mw, actor, entry,
                effect: makeRectEffect(buildRectFragment(baked)),
                nRects: baked,
                destroyId: 0, unmanagedId: 0, sizeId: 0,
            };
            pin.destroyId = actor.connect('destroy', () => {
                if (this._pins?.get(mw) === pin) {
                    this._pins.delete(mw);
                }
            });
            pin.unmanagedId = mw.connect('unmanaged', () =>
                this._pinDestroyFor(mw));
            // Resizes change the normalization — re-bake. Moves ride the
            // actor (normalized coords are actor-local).
            pin.sizeId = mw.connect('size-changed', () => this._pinRebake(mw));
            actor.add_effect(pin.effect);
            actor.queue_repaint?.();
            this._pins.set(mw, pin);
            const match = entry.tokens?.length
                ? `title [${entry.tokens.join(', ')}]`
                : `class "${entry.cls}"`;
            console.log(`Invertonator: stored area applied (${match}) ` +
                `rel=${JSON.stringify(entry.rel)} — "${mw.get_title?.() ?? '?'}"`);
            return true;
        } catch (err) {
            console.warn(`Invertonator: stored area apply failed (${err})`);
            return false;
        }
    }

    _pinDestroyFor(mw) {
        const cur = this._pins?.get(mw);
        if (!cur) {
            return;
        }
        this._pins.delete(mw);
        try { cur.actor.disconnect(cur.destroyId); } catch (e) {}
        try { cur.mw.disconnect(cur.unmanagedId); } catch (e) {}
        try { cur.mw.disconnect(cur.sizeId); } catch (e) {}
        try { cur.actor.remove_effect(cur.effect); } catch (e) {}
        try { cur.actor.queue_repaint?.(); } catch (e) {}
    }

    _pinRebake(mw) {
        const cur = this._pins?.get(mw);
        if (!cur) {
            return;
        }
        const entry = cur.entry;
        this._pinDestroyFor(mw);
        this._pinCreate(mw, entry);
    }

    _pinsDestroyAll() {
        for (const mw of [...(this._pins?.keys() ?? [])]) {
            this._pinDestroyFor(mw);
        }
    }

    _pinStartTimer() {
        this._pinStopTimer();
        this._pinTimerId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT,
            PIN_TICK_S, () => {
                this._pinsRefresh();
                return GLib.SOURCE_CONTINUE;
            });
    }

    _pinStopTimer() {
        if (this._pinTimerId) {
            try { GLib.source_remove(this._pinTimerId); } catch (e) {}
            this._pinTimerId = 0;
        }
    }

    pinDelete(i) {
        if (!this._pinEntries || i < 0 || i >= this._pinEntries.length) {
            return;
        }
        const [gone] = this._pinEntries.splice(i, 1);
        this._pinSaveEntries();
        this._pinsRefresh();
        console.log(`Invertonator: stored area forgotten ` +
            `(rel=${JSON.stringify(gone?.rel)})`);
    }

    pinsForget() {
        if (!this._pinEntries?.length) {
            return;
        }
        const n = this._pinEntries.length;
        this._pinEntries = [];
        this._pinSaveEntries();
        this._pinsRefresh();
        console.log(`Invertonator: forgot ${n} stored area(s)`);
    }

    // --- v2.3.3/v2.3.4: the drag-select overlay (two steps) ---------------
    //
    // A fullscreen St.Widget above the stage (outside uiGroup, so the
    // overlay itself is NOT inverted) with a modal grab. STEP 1: the
    // pointer drag draws a dashed rectangle. STEP 2 (on release): the
    // rectangle becomes a solid PREVIEW the user can fine-tune before
    // anything is stored — 8 resize handles (corners + edges), drag
    // inside the rect moves it, drag outside redraws from scratch, and a
    // Confirm / Redraw / Cancel bar sits beside the rect. Enter confirms,
    // R redraws, Esc (or right-click) cancels. Nothing is stored until
    // Confirm; the commit itself is unchanged (alpha.27 rel math).

    pinSelectStart() {
        if (!this._pins) {
            return;
        }
        if (this._pinSelect) {
            return;   // already selecting
        }
        if (!this._inverted) {
            console.log('Invertonator: pin ignored — inversion is off');
            Main.notify('Invertonator',
                'Turn inversion on first (Super+I), then Super+F8.');
            return;
        }
        const mw = global.display.get_focus_window?.();
        if (!mw || mw.minimized || (mw.is_override_redirect?.() ?? false)) {
            console.log('Invertonator: pin ignored — no anchor window');
            Main.notify('Invertonator',
                'No focused window to anchor the area to.');
            return;
        }

        const overlay = new St.Widget({
            reactive: true,
            layout_manager: new Clutter.FixedLayout(),
            style: 'background: rgba(0, 0, 0, 0.35);',
        });
        overlay.add_constraint(new Clutter.BindConstraint({
            source: global.stage,
            coordinate: Clutter.BindCoordinate.ALL,
        }));
        const hint = new St.Label({
            text: 'Drag a rectangle · release to fine-tune, then Confirm · Esc cancels',
            style: 'padding: 8px 14px; background: rgba(20, 20, 20, 0.85); ' +
                'color: #eeeeee; border-radius: 6px; font-size: 13px;',
        });
        hint.set_position(24, 24);
        overlay.add_child(hint);
        const box = new St.Widget({
            visible: false,
            style: 'border: 2px dashed #4fc3f7; ' +
                'background: rgba(79, 195, 247, 0.18);',
        });
        overlay.add_child(box);

        global.stage.add_child(overlay);
        global.stage.set_child_above_sibling(overlay, null);

        this._pinSelect = {
            overlay, box, mw,
            mwTitle: mw.get_title?.() ?? '',
            startX: 0, startY: 0, dragging: false, ids: [],
            phase: 'drag',      // drag | adjust | resize | move | redrag
            rect: null,         // [x, y, w, h] stage coords (adjust stage)
            rect0: null,        // rect snapshot at resize/move press
            mask: null,         // {n,s,e,w} active resize mask
            pressX: 0, pressY: 0,
            handles: [], toolbar: null, btns: [], sizeLabel: null,
            adjustBuilt: false, adjustLogged: false,
        };
        const s = this._pinSelect;
        s.ids.push(overlay.connect('button-press-event',
            (a, ev) => this._pinSelectPress(ev)));
        s.ids.push(overlay.connect('motion-event',
            (a, ev) => this._pinSelectMotion(ev)));
        s.ids.push(overlay.connect('button-release-event',
            (a, ev) => this._pinSelectRelease(ev)));
        s.ids.push(overlay.connect('key-press-event',
            (a, ev) => this._pinSelectKey(ev)));
        try {
            this._pinGrab = Main.pushModal(overlay,
                { actionMode: Shell.ActionMode.ALL });
        } catch (e) {
            this.pinSelectCancel();
            console.warn(`Invertonator: selection grab failed (${e})`);
            Main.notify('Invertonator', 'Selection grab failed — see journal.');
            return;
        }
        console.log('Invertonator: area selection started — drag a ' +
            `rectangle (anchor "${s.mwTitle}"), release to adjust, ` +
            'Esc cancels');
    }

    _pinSelectPress(ev) {
        const s = this._pinSelect;
        if (!s) {
            return Clutter.EVENT_PROPAGATE;
        }
        if (ev.get_button() !== 1) {
            this.pinSelectCancel();
            return Clutter.EVENT_STOP;
        }
        const [x, y] = ev.get_coords();
        // A press while a resize/move drag is live means the release was
        // swallowed (e.g. by a button actor under the pointer) — finalize
        // the drag at this point, then treat this press normally.
        if ((s.phase === 'resize' || s.phase === 'move') && s.dragging) {
            this._pinSelectReleaseAt(x, y);
            if (!this._pinSelect) {
                return Clutter.EVENT_STOP;
            }
        }
        s.pressX = x;
        s.pressY = y;
        if (s.phase === 'drag' || s.phase === 'redrag') {
            s.startX = x;
            s.startY = y;
            s.dragging = true;
            s.box.visible = true;
            s.box.set_position(x, y);
            s.box.set_size(1, 1);
            return Clutter.EVENT_STOP;
        }
        // adjust: press inside the rect moves it; press outside redraws.
        const [rx, ry, rw, rh] = s.rect;
        if (x >= rx && x <= rx + rw && y >= ry && y <= ry + rh) {
            s.phase = 'move';
            s.rect0 = [...s.rect];
            s.dragging = true;
            this._pinSetCursor('MOVE_ON_GRAB');
            this._pinSetToolbarActive(false);
            return Clutter.EVENT_STOP;
        }
        s.phase = 'redrag';
        s.startX = x;
        s.startY = y;
        s.dragging = true;
        this._pinStyleDragBox(s);
        s.box.visible = true;
        s.box.set_position(x, y);
        s.box.set_size(1, 1);
        return Clutter.EVENT_STOP;
    }

    _pinSelectMotion(ev) {
        const s = this._pinSelect;
        if (!s || !s.dragging) {
            return Clutter.EVENT_PROPAGATE;
        }
        const [x, y] = ev.get_coords();
        if (s.phase === 'drag' || s.phase === 'redrag') {
            s.box.set_position(Math.min(s.startX, x), Math.min(s.startY, y));
            s.box.set_size(Math.max(Math.abs(x - s.startX), 1),
                Math.max(Math.abs(y - s.startY), 1));
            return Clutter.EVENT_STOP;
        }
        if (s.phase === 'move') {
            const [sw, sh] = this._pinStageBounds(s);
            const [x0, y0, w, h] = s.rect0;
            s.rect = [
                Math.min(Math.max(x0 + x - s.pressX, 0), Math.max(sw - w, 0)),
                Math.min(Math.max(y0 + y - s.pressY, 0), Math.max(sh - h, 0)),
                w, h,
            ];
            this._pinSyncChrome(s);
            return Clutter.EVENT_STOP;
        }
        if (s.phase === 'resize') {
            const [sw, sh] = this._pinStageBounds(s);
            const [x0, y0, w0, h0] = s.rect0;
            const m = s.mask;
            let left = x0;
            let top = y0;
            let right = x0 + w0;
            let bottom = y0 + h0;
            // max/min order keeps the rect >= PIN_MIN_EDGE on every axis.
            if (m.e) {
                right = Math.min(Math.max(x, left + PIN_MIN_EDGE), sw);
            }
            if (m.w) {
                left = Math.max(Math.min(x, right - PIN_MIN_EDGE), 0);
            }
            if (m.s) {
                bottom = Math.min(Math.max(y, top + PIN_MIN_EDGE), sh);
            }
            if (m.n) {
                top = Math.max(Math.min(y, bottom - PIN_MIN_EDGE), 0);
            }
            s.rect = [left, top, right - left, bottom - top];
            this._pinSyncChrome(s);
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }

    _pinSelectRelease(ev) {
        const [x, y] = ev.get_coords();
        return this._pinSelectReleaseAt(x, y);
    }

    _pinSelectReleaseAt(x, y) {
        const s = this._pinSelect;
        if (!s || !s.dragging) {
            return Clutter.EVENT_PROPAGATE;
        }
        s.dragging = false;
        if (s.phase === 'drag' || s.phase === 'redrag') {
            const bx = Math.min(s.startX, x);
            const by = Math.min(s.startY, y);
            const bw = Math.abs(x - s.startX);
            const bh = Math.abs(y - s.startY);
            if (bw < PIN_MIN_EDGE || bh < PIN_MIN_EDGE) {
                if (s.phase === 'drag') {
                    this.pinSelectCancel();
                    Main.notify('Invertonator',
                        'Selection too small — nothing stored.');
                    return Clutter.EVENT_STOP;
                }
                // A missed re-drag keeps the previous rect — back to it.
                this._pinEnterAdjust(s.rect[0], s.rect[1],
                    s.rect[2], s.rect[3]);
                return Clutter.EVENT_STOP;
            }
            this._pinEnterAdjust(bx, by, bw, bh);
            return Clutter.EVENT_STOP;
        }
        // resize / move finished → back to the confirmable preview
        this._pinEnterAdjust(s.rect[0], s.rect[1], s.rect[2], s.rect[3]);
        return Clutter.EVENT_STOP;
    }

    _pinSelectKey(ev) {
        const s = this._pinSelect;
        const sym = ev.get_key_symbol();
        if (sym === Clutter.KEY_Escape) {
            this.pinSelectCancel();
            return Clutter.EVENT_STOP;
        }
        if (!s || s.phase === 'drag' || s.phase === 'redrag') {
            return Clutter.EVENT_PROPAGATE;
        }
        if (sym === Clutter.KEY_Return || sym === Clutter.KEY_KP_Enter
            || sym === Clutter.KEY_ISO_Enter) {
            this._pinConfirm();
            return Clutter.EVENT_STOP;
        }
        if (sym === Clutter.KEY_r || sym === Clutter.KEY_R) {
            this._pinRestartDrag();
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    }

    // STEP 2 — the adjustable preview. Builds the chrome once, re-uses it
    // on every re-entry (resize release, move release, missed re-drag).
    _pinEnterAdjust(x, y, w, h) {
        const s = this._pinSelect;
        if (!s) {
            return;
        }
        s.phase = 'adjust';
        s.rect = [x, y, w, h];
        s.rect0 = null;
        s.mask = null;
        s.dragging = false;
        if (!s.adjustBuilt) {
            this._pinBuildAdjustUI(s);
            s.adjustBuilt = true;
        }
        this._pinSetCursor('DEFAULT');
        this._pinStyleAdjustBox(s);
        this._pinSetToolbarActive(true);
        this._pinSyncChrome(s);
        if (!s.adjustLogged) {
            s.adjustLogged = true;
            console.log('Invertonator: area preview — drag the handles or ' +
                'the rect to adjust, then Confirm (Enter); R redraws, ' +
                'Esc cancels');
        }
    }

    _pinHandlePress(h, ev) {
        const s = this._pinSelect;
        if (!s) {
            return Clutter.EVENT_STOP;
        }
        if (ev.get_button() !== 1) {
            this.pinSelectCancel();
            return Clutter.EVENT_STOP;
        }
        const [x, y] = ev.get_coords();
        s.pressX = x;
        s.pressY = y;
        s.phase = 'resize';
        s.rect0 = [...s.rect];
        s.mask = h.mask;
        s.dragging = true;
        this._pinSetCursor(h.cursor);
        this._pinSetToolbarActive(false);
        return Clutter.EVENT_STOP;
    }

    _pinBuildAdjustUI(s) {
        // 8 handles — corners + edges — in syncChrome anchor order.
        const masks = [
            [{ n: 1, w: 1 }, 'NW_RESIZE'],
            [{ n: 1 }, 'N_RESIZE'],
            [{ n: 1, e: 1 }, 'NE_RESIZE'],
            [{ e: 1 }, 'E_RESIZE'],
            [{ s: 1, e: 1 }, 'SE_RESIZE'],
            [{ s: 1 }, 'S_RESIZE'],
            [{ s: 1, w: 1 }, 'SW_RESIZE'],
            [{ w: 1 }, 'W_RESIZE'],
        ];
        for (const [mask, cursor] of masks) {
            const hd = new St.Widget({
                reactive: true,
                style: 'background: #ffffff; border: 2px solid #1e88e5; ' +
                    'border-radius: 3px;',
            });
            hd.set_size(PIN_HANDLE, PIN_HANDLE);
            hd.connect('button-press-event',
                (a, ev) => this._pinHandlePress({ mask, cursor }, ev));
            s.overlay.add_child(hd);
            s.handles.push(hd);
        }
        // Size chip — non-reactive so presses fall through to the rect
        // (an inside-rect press means move).
        s.sizeLabel = new St.Label({
            reactive: false,
            style: 'padding: 3px 8px; background: rgba(20, 20, 20, 0.85); ' +
                'color: #eeeeee; border-radius: 4px; font-size: 11px;',
        });
        s.overlay.add_child(s.sizeLabel);
        // Confirm / Redraw / Cancel bar. Button presses must not reach the
        // overlay handler (it would read them as redraw / move presses).
        const tb = new St.BoxLayout({
            reactive: true,
            style: 'padding: 6px 8px; background: rgba(20, 20, 20, 0.92); ' +
                'border-radius: 8px;',
        });
        tb.connect('button-press-event', () => Clutter.EVENT_STOP);
        const mkBtn = (text, cb, accent) => {
            const b = new St.Button({
                child: new St.Label({ text }),
                style: 'padding: 5px 14px; border-radius: 6px; ' +
                    `background: ${accent}; color: #ffffff; ` +
                    'font-size: 12px; font-weight: bold;',
            });
            b.connect('clicked', cb);
            tb.add_child(b);
            s.btns.push(b);
        };
        mkBtn('Confirm area', () => this._pinConfirm(), '#2e7d32');
        mkBtn('Redraw', () => this._pinRestartDrag(), '#546e7a');
        mkBtn('Cancel', () => this.pinSelectCancel(), '#616161');
        s.toolbar = tb;
        s.overlay.add_child(tb);
    }

    _pinStyleDragBox(s) {
        s.box.set_style('border: 2px dashed #4fc3f7; ' +
            'background: rgba(79, 195, 247, 0.18);');
        for (const hd of s.handles) {
            hd.visible = false;
        }
        if (s.sizeLabel) {
            s.sizeLabel.visible = false;
        }
        if (s.toolbar) {
            s.toolbar.visible = false;
        }
    }

    _pinStyleAdjustBox(s) {
        s.box.set_style('border: 2px solid #4fc3f7; ' +
            'background: rgba(79, 195, 247, 0.12);');
        s.box.visible = true;
        for (const hd of s.handles) {
            hd.visible = true;
        }
        if (s.sizeLabel) {
            s.sizeLabel.visible = true;
        }
        if (s.toolbar) {
            s.toolbar.visible = true;
        }
    }

    // One place repositions EVERYTHING after any rect change.
    _pinSyncChrome(s) {
        const [x, y, w, h] = s.rect;
        s.box.set_position(x, y);
        s.box.set_size(w, h);
        const H2 = Math.round(PIN_HANDLE / 2);
        const anchors = [
            [x, y], [x + w / 2, y], [x + w, y],
            [x + w, y + h / 2],
            [x + w, y + h], [x + w / 2, y + h], [x, y + h],
            [x, y + h / 2],
        ];
        s.handles.forEach((hd, i) => {
            const [cx, cy] = anchors[i];
            hd.set_position(Math.round(cx) - H2, Math.round(cy) - H2);
        });
        if (s.sizeLabel) {
            s.sizeLabel.text = `${Math.round(w)} × ${Math.round(h)} px`;
            const [, natH] = s.sizeLabel.get_preferred_height(-1);
            if (y > natH + PIN_UI_GAP) {
                s.sizeLabel.set_position(Math.round(x),
                    Math.round(y - natH - 6));
            } else {
                s.sizeLabel.set_position(Math.round(x + 6),
                    Math.round(y + 6));
            }
        }
        if (s.toolbar) {
            const [, natW] = s.toolbar.get_preferred_width(-1);
            const [, natTH] = s.toolbar.get_preferred_height(natW);
            const [sw, sh] = this._pinStageBounds(s);
            let tx = Math.min(Math.max(x + w / 2 - natW / 2, PIN_MARGIN),
                Math.max(sw - natW - PIN_MARGIN, PIN_MARGIN));
            let ty = y + h + PIN_UI_GAP;
            if (ty + natTH > sh - PIN_MARGIN) {
                ty = y - natTH - PIN_UI_GAP;   // no room below → above
            }
            ty = Math.min(Math.max(ty, PIN_MARGIN),
                Math.max(sh - natTH - PIN_MARGIN, PIN_MARGIN));
            s.toolbar.set_position(Math.round(tx), Math.round(ty));
        }
    }

    // Buttons only clickable while the preview is idle — a click during a
    // live drag must not steal the release event the drag needs.
    _pinSetToolbarActive(on) {
        const s = this._pinSelect;
        if (!s || !s.toolbar) {
            return;
        }
        s.toolbar.reactive = on;
        for (const b of s.btns) {
            b.reactive = on;
        }
    }

    _pinConfirm() {
        const s = this._pinSelect;
        if (!s || !s.rect || s.phase === 'drag' || s.phase === 'redrag') {
            return;
        }
        const [x, y, w, h] = s.rect;
        if (w < PIN_MIN_EDGE || h < PIN_MIN_EDGE) {
            Main.notify('Invertonator', 'Selection too small — nothing stored.');
            return;   // stay in adjust — the user can fix or redraw
        }
        this._pinSelectCommit(x, y, w, h);
    }

    _pinRestartDrag() {
        const s = this._pinSelect;
        if (!s) {
            return;
        }
        s.phase = 'drag';
        s.rect = null;
        s.rect0 = null;
        s.mask = null;
        s.dragging = false;
        this._pinSetCursor('DEFAULT');
        this._pinStyleDragBox(s);
        s.box.visible = false;
        console.log('Invertonator: area selection — redraw requested');
    }

    _pinSetCursor(name) {
        try {
            const c = Meta.Cursor?.[name];
            if (c !== undefined && c !== null) {
                global.display.set_cursor(c);
            }
        } catch (e) {}
    }

    _pinStageBounds(s) {
        try {
            const [w, h] = global.display.get_size();
            if (w > 0 && h > 0) {
                return [w, h];
            }
        } catch (e) {}
        return [
            s.overlay.width || global.stage.width || 1920,
            s.overlay.height || global.stage.height || 1080,
        ];
    }

    _pinSelectCommit(x, y, w, h) {
        const s = this._pinSelect;
        // Input first — never leave the user grabbed while we compute.
        this._pinSelectTeardown();
        try {
            let fr = null;
            try {
                fr = s.mw?.get_frame_rect?.() ?? null;
            } catch (e) {}
            if (!fr || fr.width <= 0) {
                fr = global.display.get_focus_window?.()
                    ?.get_frame_rect?.() ?? null;
            }
            if (!fr || fr.width <= 0) {
                Main.notify('Invertonator', 'Anchor window gone — nothing stored.');
                return;
            }
            const rel = [
                Math.min(Math.max((x - fr.x) / fr.width, 0), 1),
                Math.min(Math.max((y - fr.y) / fr.height, 0), 1),
                Math.min(Math.max((x + w - fr.x) / fr.width, 0), 1),
                Math.min(Math.max((y + h - fr.y) / fr.height, 0), 1),
            ].map(v => Math.round(v * 10000) / 10000);
            if (rel[2] - rel[0] < PIN_MIN_REL || rel[3] - rel[1] < PIN_MIN_REL) {
                Main.notify('Invertonator', 'Selection too small — nothing stored.');
                return;
            }
            const tokens = this._pinTokensFromTitle(s.mwTitle);
            const entry = {
                rel,
                cls: (s.mw?.get_wm_class?.() ?? '').toLowerCase(),
                tokens,
                ts: Date.now(),
            };
            // Re-storing the exact same area replaces the old pin instead
            // of piling up duplicates.
            const sig = JSON.stringify([entry.rel, entry.tokens, entry.cls]);
            this._pinEntries = this._pinEntries.filter(e2 =>
                JSON.stringify([e2.rel, e2.tokens ?? [], e2.cls ?? '']) !== sig);
            this._pinEntries.push(entry);
            while (this._pinEntries.length > PIN_MAX_ENTRIES) {
                this._pinEntries.shift();
            }
            this._pinSaveEntries();
            this._pinsRefresh();
            const match = tokens.length
                ? `title tokens [${tokens.join(', ')}]`
                : `class "${entry.cls}"`;
            console.log(`Invertonator: area stored — match ${match}, ` +
                `rel=${JSON.stringify(entry.rel)}`);
            Main.notify('Invertonator',
                'Area stored — it re-applies whenever a matching window shows up.');
        } catch (err) {
            console.warn(`Invertonator: storing area failed (${err})`);
        }
    }

    pinSelectCancel(silent = false) {
        if (!this._pinSelect) {
            return;
        }
        this._pinSelectTeardown();
        if (!silent) {
            console.log('Invertonator: area selection cancelled');
        }
    }

    _pinSelectTeardown() {
        const s = this._pinSelect;
        if (!s) {
            return;
        }
        this._pinSetCursor('DEFAULT');
        this._pinSelect = null;
        for (const id of s.ids) {
            try { s.overlay.disconnect(id); } catch (e) {}
        }
        // GNOME 45+: popModal wants the grab handle returned by pushModal;
        // destroying the grabbed actor releases any surviving grab anyway.
        try {
            Main.popModal(this._pinGrab ?? s.overlay);
        } catch (e) {
            try { this._pinGrab?.dismiss?.(); } catch (e2) {}
        }
        this._pinGrab = null;
        try { s.overlay.destroy(); } catch (e) {}
    }

    accelFor(key, fallback) {
        try {
            const accels = this._settings?.get_strv(key);
            if (accels && accels.length > 0) {
                return accels[0]
                    .replace(/<Super>/g, 'Super+')
                    .replace(/<Alt>/g, 'Alt+')
                    .replace(/<Ctrl>/g, 'Ctrl+')
                    .replace(/<Primary>/g, 'Ctrl+')
                    .replace(/<Shift>/g, 'Shift+');
            }
        } catch (e) {}
        return fallback;
    }

    setInverted(on) {
        if (on === this._inverted) {
            return;
        }
        if (this._effect === null) {
            console.error('Invertonator: no shader effect — cannot invert');
            return;
        }
        try {
            const group = Main.layoutManager.uiGroup;
            if (on) {
                group.add_effect(this._effect);
            } else {
                group.remove_effect(this._effect);
            }
        } catch (e) {
            console.error(`Invertonator: effect toggle failed (${e})`);
            return;
        }
        this._inverted = on;
        // v2.1: keep fullscreen video out of direct scanout while the
        // shaders are active (see _setUnredirectDisabled).
        this._setUnredirectDisabled(on);
        if (!on) {
            // Holes exist only relative to the global pass; leaving one on
            // would make that window the ONLY inverted thing on screen.
            this.clearHoles();
            this._mediaStop();
            // v2.3.3: pin effects go with the shaders; the STORED entries
            // stay in gsettings for the next enable.
            this._pinStopTimer();
            this._pinsDestroyAll();
        } else {
            // Windows already fullscreen when inversion comes on (user was
            // watching video, then inverted): give the first media walk a
            // beat to claim element rects before the whole-window fallback
            // fires — no full-window flash that would re-invert chrome.
            if (this._media?.running) {
                GLib.timeout_add(GLib.PRIORITY_DEFAULT, MEDIA_FULLSCREEN_SYNC_MS,
                    () => {
                        if (this._inverted && this._fullscreenWatch) {
                            this._syncFullscreenHoles();
                        }
                        return GLib.SOURCE_REMOVE;
                    });
            } else {
                this._syncFullscreenHoles();
            }
            this._mediaStart();
            // v2.3.3: stored pins come back with the inversion — reload
            // (settings may have changed since), scan, and start the cheap
            // title-matching tick.
            this._pinLoadEntries();
            this._pinsRefresh();
            this._pinStartTimer();
        }
        try {
            this._settings.set_boolean('start-enabled', on);
        } catch (e) {}
        if (this._indicator) {
            this._indicator.setState(on, this._holes ? this._holes.size : 0);
        }
        console.log(`Invertonator: inversion ${on ? 'ON' : 'OFF'}`);
    }
}
