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
//   v2 holes (alpha.20): F7 "pierce" — the Windows Magnifier analog.
//   A hole is the SAME inversion shader attached a SECOND time, to the
//   picked window actor itself. Inversion is an involution (1-x), so a
//   window its own effect paints inverted and uiGroup's global pass
//   paints again comes out TRUE COLOR — a hole punched through the
//   global inversion covering exactly that window (moves, resizes and
//   fullscreening follow automatically; no rectangle tracking, no
//   overlay windows). F7 picks the topmost window under the pointer;
//   press again over the same window to close the hole; turning
//   inversion off clears every hole. (Windows additionally auto-holes
//   media via UIA — the AT-SPI2 auto-media analog is v2.1.)
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
                'Pierce window under pointer (F7)'
            );
            pierceItem.connect('activate', () => this._ext.pierce());
            this.menu.addMenuItem(pierceItem);

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
    }
);

export default class InvertonatorExtension extends Extension {
    enable() {
        this._settings = this.getSettings(SCHEMA);
        this._inverted = false;
        this._holes = new Map(); // Meta.Window -> {actor, mw, effect, destroyId}

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

        if (this._settings.get_boolean('start-enabled')) {
            this.setInverted(true);
        }
        console.log(`Invertonator ${this.metadata.version} enabled — Super+I invert, F7 pierce hole`);
    }

    disable() {
        // Colors must come back before we vanish — never leave a user
        // staring at an inverted screen with the toggle gone. Also clears
        // every hole (they are meaningless without the global pass).
        this.setInverted(false);
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
            if (!mw || mw.minimized || !mw.showing_on_its_workspace()) {
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
            this._removeHole(top.mw);
            console.log(`Invertonator: hole OFF (${cls}) "${title}"`);
            Main.notify('Invertonator', `Hole OFF: ${title}`);
        } else {
            this._addHole(top.actor, top.mw);
            console.log(`Invertonator: hole ON (${cls}) "${title}" — ` +
                'window re-inverts the global pass = true color');
            Main.notify('Invertonator', `Hole ON: ${title}`);
        }
        this._indicator?.setState(this._inverted, this._holes.size);
    }

    clearHoles() {
        if (!this._holes || this._holes.size === 0) {
            return;
        }
        const n = this._holes.size;
        for (const mw of [...this._holes.keys()]) {
            this._removeHole(mw);
        }
        console.log(`Invertonator: cleared ${n} hole(s)`);
        this._indicator?.setState(this._inverted, this._holes.size);
    }

    _addHole(actor, mw) {
        const hole = { actor, mw, effect: makeInvertEffect(), destroyId: 0 };
        hole.destroyId = actor.connect('destroy', () => {
            // Window closed with its hole still on — the actor (and its
            // effects) die with it; just drop the bookkeeping.
            if (this._holes.get(mw) === hole) {
                this._holes.delete(mw);
                this._indicator?.setState(this._inverted, this._holes.size);
            }
        });
        actor.add_effect(hole.effect);
        this._holes.set(mw, hole);
    }

    _removeHole(mw) {
        const hole = this._holes.get(mw);
        if (!hole) {
            return;
        }
        try {
            hole.actor.remove_effect(hole.effect);
            if (hole.destroyId) {
                hole.actor.disconnect(hole.destroyId);
            }
        } catch (e) {}
        this._holes.delete(mw);
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
        if (!on) {
            // Holes exist only relative to the global pass; leaving one on
            // would make that window the ONLY inverted thing on screen.
            this.clearHoles();
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
