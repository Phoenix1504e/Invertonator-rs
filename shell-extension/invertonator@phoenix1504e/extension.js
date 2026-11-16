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
        rebuildHoles(infos) {
            this._holeSub.menu.removeAll();
            this._holeSub.label.text = `Holes: ${infos.length}`;
            if (infos.length === 0) {
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

        if (this._settings.get_boolean('start-enabled')) {
            this.setInverted(true);
        }
        console.log(`Invertonator ${this.metadata.version} enabled — ` +
            'Super+I invert · Super+F7 pierce · fullscreen auto-hole');
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
            this._holes ? this.listHoles() : []);
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
        } else {
            // Windows already fullscreen when inversion comes on (user
            // was watching video, then inverted) — hole them now.
            this._syncFullscreenHoles();
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
