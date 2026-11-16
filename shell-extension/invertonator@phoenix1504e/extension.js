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
//     * Hole survival: if Mutter recreates a window actor (remap paths),
//       the hole reattaches to the new actor via an idle re-check;
//       window close drops its hole cleanly (unmanaged safety net).
//     * Direct-scanout guard: inversion disables unredirect/direct
//       scanout where the Mutter API exists, so a fullscreen video can
//       never bypass the compositor shaders.
//     * Pick includes override-redirect windows (context menus,
//       tooltips) — pierce those too.
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

// v2.2 element media lock-on budgets (mirrors the Windows tracker's
// cadence/evidence design — D-Bus calls cost main-loop time, so every
// number here is a jank guard).
const MEDIA_MAX_HOLES = 6;          // hole pool cap (Windows analog: pool)
const MEDIA_MIN_EDGE = 48;          // icons/thumbnails stay inverted
const MEDIA_WALK_INTERVAL_S = 3;    // cached-walk cadence
const MEDIA_WALK_BUDGET = 240;      // max D-Bus calls per walk
const MEDIA_WALK_CHUNK_MS = 8;      // max block per idle chunk
const MEDIA_CACHE_CAP = 1200;       // cached subtree nodes
const MEDIA_REFRESH_MIN_MS = 150;   // extents refresh throttle (scroll)
const MEDIA_WALK_MIN_MS = 1200;     // min gap between full walks
const MEDIA_APP_BLACKLIST_MS = 60000;// hung-app protection
const MEDIA_SLOW_APP_MS = 1200;     // probe slower than this → blacklist
const MEDIA_MATCH_DIST = 96;        // px — rect churn matching window

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

            // v2.1: per-hole manager — one row per hole, click to close it.
            this._holeSub = new PopupMenu.PopupSubMenuMenuItem('Holes: 0');
            this.menu.addMenuItem(this._holeSub);
            this.rebuildHoles([]);

            const clearItem = new PopupMenu.PopupMenuItem('Clear all holes');
            clearItem.connect('activate', () => this._ext.clearHoles());
            this.menu.addMenuItem(clearItem);
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
        // v2.2 element media lock-on — walker + rect-hole state
        this._media = {
            atspi: null, roleMap: null, atspiFailed: false, initStarted: false,
            running: false, timerId: 0, walkTimerId: 0, walkScheduled: false,
            chunkId: 0, focusTimerId: 0, walkActive: false, callBudget: 0,
            queue: [], seen: null, found: [], targetMw: null,
            apps: null, appsTs: 0, slowApps: new Map(), cache: new Map(),
            holes: [], lastWalk: 0, lastRefresh: 0, publishedCount: -1,
            hinted: false, eventCbs: [],
        };
        this._focusId = 0;

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

        // v2.1: fullscreen auto-hole — watch every window's fullscreen
        // state (existing windows now, future windows via window-created).
        this._windowCreatedId = global.display.connect('window-created',
            (display, mw) => this._watchWindow(mw));
        this._watchAllWindows();

        // v2.2: element media lock-on — focus changes retarget the walker
        this._focusId = global.display.connect('notify::focus-window', () =>
            this._mediaFocusChanged());
        this._media.initStarted = true;
        this._mediaInitAtspi();

        if (this._settings.get_boolean('start-enabled')) {
            this.setInverted(true);
        }
        console.log(`Invertonator ${this.metadata.version} enabled — ` +
            'Super+I invert · Super+F7 pierce · fullscreen auto-hole · media lock-on');
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
        this._indicator.destroy();
        this._indicator = null;
        this._effect = null;
        this._holes = null;
        this._fullscreenWatch = null;
        this._unredirectDisabled = false;
        this._media = null;
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
        this._indicator?.rebuildHoles(
            this._holes ? this.listHoles() : [],
            this._media ? this._media.holes.length : 0);
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
        if (isFs && setting && !this._holes.has(mw)) {
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

    // --- v2.2: element media lock-on (AT-SPI2 rect holes) ----------------
    //
    // The Windows backend walks the foreground UIA tree and punches holes
    // at media elements. The Wayland analog: walk the FOCUSED window's
    // AT-SPI2 tree (image/video/animation roles), calibrate element
    // extents against the window frame, and punch RECT holes — a clipped
    // Clutter.Clone of the window actor carrying the invert effect. The
    // clone is an IDENTITY overlay of the source (translation cancels the
    // container position), so the math is invariant to the actor's scale
    // factor; the container's clip alone selects the media rect, and the
    // double-invert involution makes exactly that rect true color while
    // everything else stays inverted. Inserted directly above the source
    // window actor in window_group, so overlapping windows cover the hole
    // (correct stacking semantics).
    //
    // Evidence rules (alpha.11/14 analogs): holes belong to the focused
    // window's tree (focus change = context gone → holes cleared), events
    // (bounds-changed = scroll) refresh extents cheaply, full walks are
    // budgeted/chunked so D-Bus never janks the shell, and a slow app is
    // blacklisted for a minute.

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
        for (const t of types) {
            try {
                const cb = () => this._mediaOnEvent(t);
                m.atspi.EventListener.add_callback(cb, t);
                m.eventCbs.push({ cb, t });
            } catch (e) {
                console.warn(`Invertonator: AT-SPI event '${t}' unavailable (${e})`);
            }
        }
    }

    _mediaOnEvent(type) {
        const m = this._media;
        if (!m?.running || !this._inverted) {
            return;
        }
        const now = Date.now();
        // Tree-shape events → full (budgeted, cached) walk; move events →
        // cheap extents-only refresh (the scroll-follow path).
        if (type.startsWith('document:') || type === 'object:children-changed') {
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
        for (const id of [m.timerId, m.walkTimerId, m.focusTimerId, m.chunkId]) {
            if (id) {
                try { GLib.source_remove(id); } catch (e) {}
            }
        }
        m.timerId = m.walkTimerId = m.focusTimerId = m.chunkId = 0;
        m.walkScheduled = false;
        m.walkActive = false;
        m.queue = [];
        this._mediaClearHoles();
    }

    _mediaShutdown() {
        const m = this._media;
        if (!m) {
            return;
        }
        this._mediaStop();
        for (const { cb, t } of m.eventCbs) {
            try {
                m.atspi?.EventListener?.deregister_callback?.(cb, t);
            } catch (e) {}
        }
        m.eventCbs = [];
        m.cache.clear();
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
        try {
            const iface = Gio.Settings.new('org.gnome.desktop.interface');
            if (!iface.get_boolean('toolkit-accessibility')) {
                m.hinted = true;
                console.log('Invertonator: toolkit-accessibility is OFF — ' +
                    'app trees may come up empty; enable with: gsettings set ' +
                    'org.gnome.desktop.interface toolkit-accessibility true');
            }
        } catch (e) {}
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

    _mediaApiMiss(name, e) {
        const m = this._media;
        if (!m.apiMissed) {
            m.apiMissed = true;
            console.warn(`Invertonator: AT-SPI API miss '${name}' (${e}) — ` +
                'element lock-on degraded; report the journal line');
        }
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
        const mw = global.display.get_focus_window?.();
        if (!mw || (mw.get_pid?.() ?? 0) <= 0 || mw.minimized) {
            this._mediaPublish([]);
            return;
        }
        m.walkActive = true;
        m.callBudget = MEDIA_WALK_BUDGET;
        m.found = [];
        m.targetMw = mw;
        this._mediaResolveFrames(mw, (roots) => {
            if (!m.walkActive) {
                return;
            }
            if (!roots || roots.length === 0) {
                m.walkActive = false;
                this._mediaPublish([]);
                return;
            }
            m.queue = roots;
            m.seen = new Set(roots.map(r => r.acc));
            this._mediaWalkChunk();
        });
    }

    // Map the focused Meta.Window to its AT-SPI2 application (by PID, with
    // wm_class name as fallback), return its top-level frames pre-calibrated:
    // cal = {k, ex, ey} — logical(p) = frameRect.xy + (p - frameExt.xy) * k.
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
        done(roots);
    }

    // Budgeted, chunked BFS. Cached nodes cost ZERO D-Bus calls (role +
    // children reused from the previous walk — the Windows "cached walk"
    // analog); only new nodes and live media extents pay.
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
            const cached = m.cache.get(acc);
            let role;
            if (cached) {
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
                    } catch (e) {}
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
                    m.cache.set(acc, { role, kids, ts: Date.now() });
                }
                for (const k of kids) {
                    if (!m.seen.has(k)) {
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
            m.walkActive = false;
            this._mediaPublish([]);
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
            this._mediaPublishFromFound();
        }
    }

    _mediaPublishFromFound() {
        const m = this._media;
        const mw = m.targetMw;
        if (!mw || m.found.length === 0) {
            this._mediaPublish([]);
            return;
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
            // Clip to the window frame; drop anything mostly outside it.
            const cx = Math.max(x, fr.x);
            const cy = Math.max(y, fr.y);
            const cw = Math.min(x + w, fr.x + fr.width) - cx;
            const ch = Math.min(y + h, fr.y + fr.height) - cy;
            if (cw < MEDIA_MIN_EDGE || ch < MEDIA_MIN_EDGE) {
                continue;
            }
            entries.push({
                mw, acc: f.acc, frameAcc: f.frameAcc, cal: f.cal, kind: f.kind,
                x: cx, y: cy, w: cw, h: ch,
            });
        }
        this._mediaPublish(entries);
    }

    // Diff-and-rebuild the rect hole pool. Matching by element identity
    // first (acc object), then by kind+window+proximity — scrolling keeps
    // the same hole object alive and just moves it (no churn).
    _mediaPublish(entries) {
        const m = this._media;
        if (!m) {
            return;
        }
        entries = entries.filter(e => e.mw && !this._holes?.has(e.mw));
        const used = new Set();
        const next = [];
        for (const e of entries) {
            let match = null;
            for (const h of m.holes) {
                if (!used.has(h) && h.acc === e.acc) {
                    match = h;
                    break;
                }
            }
            if (!match) {
                let best = MEDIA_MATCH_DIST;
                for (const h of m.holes) {
                    if (used.has(h) || h.kind !== e.kind || h.mw !== e.mw) {
                        continue;
                    }
                    const d = Math.hypot(h.rect.x - e.x, h.rect.y - e.y);
                    if (d < best) {
                        best = d;
                        match = h;
                    }
                }
            }
            if (match) {
                used.add(match);
                this._elementHoleUpdate(match, e);
                next.push(match);
            } else {
                const h = this._elementHoleCreate(e);
                if (h) {
                    next.push(h);
                }
            }
        }
        for (const h of m.holes) {
            if (!used.has(h)) {
                this._elementHoleDestroy(h);
            }
        }
        const countChanged = next.length !== m.publishedCount;
        m.holes = next;
        m.publishedCount = entries.length;
        if (countChanged) {
            const kinds = [...new Set(next.map(h => h.kind))].join(',');
            console.log(`Invertonator: media lock-on ${next.length} rect(s)` +
                (next.length ? ` (${kinds})` : ''));
            this._refreshHoleUI();
        }
    }

    // THE rect hole: container (clip = the media rect) + identity clone of
    // the window actor (translation = -container pos cancels the container's
    // transform — content overlays the real window pixel-for-pixel at ANY
    // scale) + the invert effect. Global pass re-inverts it → true color.
    _elementHoleCreate(e) {
        try {
            const actor = e.mw.get_compositor_private?.();
            if (!actor) {
                return null;
            }
            const container = new Clutter.Actor({ reactive: false });
            let clone;
            try {
                clone = Clutter.Clone.new(actor);
            } catch (e) {
                clone = new Clutter.Clone({ source: actor });
            }
            clone.reactive = false;
            container.set_position(e.x, e.y);
            container.set_size(e.w, e.h);
            container.set_clip(0, 0, e.w, e.h);
            clone.translation_x = -e.x;
            clone.translation_y = -e.y;
            container.add_child(clone);
            container.add_effect(makeInvertEffect());
            global.window_group.insert_child_above(container, actor);
            const fr = e.mw.get_frame_rect();
            const hole = {
                mw: e.mw, acc: e.acc, frameAcc: e.frameAcc, cal: e.cal,
                kind: e.kind, rect: { x: e.x, y: e.y, w: e.w, h: e.h },
                rel: { x: e.x - fr.x, y: e.y - fr.y },
                container, clone,
                posId: e.mw.connect('position-changed', () =>
                    this._elementHoleReposition(hole)),
                sizeId: e.mw.connect('size-changed', () =>
                    this._elementHoleReposition(hole)),
            };
            return hole;
        } catch (err) {
            console.warn(`Invertonator: element hole create failed (${err})`);
            return null;
        }
    }

    _elementHoleUpdate(h, e) {
        try {
            const fr = e.mw.get_frame_rect();
            h.acc = e.acc;
            h.cal = e.cal;
            h.kind = e.kind;
            h.rect = { x: e.x, y: e.y, w: e.w, h: e.h };
            h.rel = { x: e.x - fr.x, y: e.y - fr.y };
            h.container.set_position(e.x, e.y);
            h.container.set_size(e.w, e.h);
            h.container.set_clip(0, 0, e.w, e.h);
            h.clone.translation_x = -e.x;
            h.clone.translation_y = -e.y;
        } catch (err) {}
    }

    _elementHoleReposition(hole) {
        try {
            const fr = hole.mw.get_frame_rect();
            const mx = fr.x + hole.rel.x;
            const my = fr.y + hole.rel.y;
            hole.container.set_position(mx, my);
            hole.clone.translation_x = -mx;
            hole.clone.translation_y = -my;
        } catch (e) {}
    }

    _elementHoleDestroy(hole) {
        try { hole.mw.disconnect(hole.posId); } catch (e) {}
        try { hole.mw.disconnect(hole.sizeId); } catch (e) {}
        try { hole.container.destroy(); } catch (e) {}
    }

    _mediaClearHoles() {
        const m = this._media;
        if (!m) {
            return;
        }
        for (const h of m.holes) {
            this._elementHoleDestroy(h);
        }
        m.holes = [];
        m.publishedCount = 0;
    }

    _mediaDropForWindow(mw) {
        const m = this._media;
        if (!m || m.holes.length === 0) {
            return;
        }
        const keep = [];
        let dropped = 0;
        for (const h of m.holes) {
            if (h.mw === mw) {
                this._elementHoleDestroy(h);
                dropped++;
            } else {
                keep.push(h);
            }
        }
        if (dropped > 0) {
            m.holes = keep;
            this._refreshHoleUI();
        }
    }

    // Scroll-follow: cheap extents re-query of the LIVE holes only (≤6
    // elements × 2 calls), no tree walk. Self-correcting calibration as in
    // publish (Δ between element and frame cancels window movement).
    _mediaRefreshExtents() {
        const m = this._media;
        if (!m?.running || m.walkActive || m.holes.length === 0) {
            return;
        }
        const coord = m.atspi.CoordType?.SCREEN ?? 0;
        const entries = [];
        for (const h of m.holes) {
            try {
                const fr = h.mw.get_frame_rect();
                const comp = h.acc.getComponent?.();
                const ext = comp?.get_extents?.(coord);
                const fcomp = h.frameAcc?.getComponent?.();
                const fext = fcomp?.get_extents?.(coord);
                if (!ext || ext.width <= 0) {
                    continue;
                }
                let ex = h.cal.ex, ey = h.cal.ey, k = h.cal.k;
                if (fext && fext.width > 0) {
                    const kNow = fr.width / fext.width;
                    if (kNow > 0.2 && kNow < 5) {
                        k = kNow;
                    }
                    ex = fext.x;
                    ey = fext.y;
                }
                const x = fr.x + (ext.x - ex) * k;
                const y = fr.y + (ext.y - ey) * k;
                entries.push({
                    mw: h.mw, acc: h.acc, frameAcc: h.frameAcc,
                    cal: { k, ex, ey }, kind: h.kind,
                    x, y, w: ext.width * k, h: ext.height * k,
                });
            } catch (e) {}
        }
        this._mediaPublish(entries);
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
        } else {
            // Windows already fullscreen when inversion comes on (user
            // was watching video, then inverted) — hole them now.
            this._syncFullscreenHoles();
            this._mediaStart();
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
