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
//   v2 roadmap (Wayland true-color holes): invert each window actor
//   individually and leave the picked "hole" window (pointer-pick, the
//   F7 analog) un-inverted — the Wayland analog of the Windows
//   Magnifier holes and the X11 XComposite overlay design.
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
        }

        setState(inverted) {
            this._item.label.text = inverted ? 'Inversion: ON' : 'Inversion: OFF';
        }
    }
);

export default class InvertonatorExtension extends Extension {
    enable() {
        this._settings = this.getSettings(SCHEMA);
        this._inverted = false;

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

        try {
            Main.wm.addKeybinding(
                'toggle-invert',
                this._settings,
                Meta.KeyBindingFlags.NONE,
                Shell.ActionMode.ALL,
                () => this.toggle()
            );
        } catch (e) {
            console.error(`Invertonator: keybinding registration failed (${e}) — ` +
                'the panel menu still works');
        }

        if (this._settings.get_boolean('start-enabled')) {
            this.setInverted(true);
        }
        console.log(`Invertonator ${this.metadata.version} enabled — Super+I or panel menu`);
    }

    disable() {
        // Colors must come back before we vanish — never leave a user
        // staring at an inverted screen with the toggle gone.
        this.setInverted(false);
        try {
            Main.wm.removeKeybinding('toggle-invert');
        } catch (e) {}
        this._indicator.destroy();
        this._indicator = null;
        this._effect = null;
        this._settings = null;
    }

    toggle() {
        this.setInverted(!this._inverted);
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
        try {
            this._settings.set_boolean('start-enabled', on);
        } catch (e) {}
        if (this._indicator) {
            this._indicator.setState(on);
        }
        console.log(`Invertonator: inversion ${on ? 'ON' : 'OFF'}`);
    }
}
