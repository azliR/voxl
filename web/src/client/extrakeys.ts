// Extra keys: while the on-screen keyboard is up on a touch device, a bar above it
// offers the keys phone keyboards lack (Esc, Tab, Ctrl, Cmd or Win, Alt or Option,
// Shift, arrows, Home/End, Page Up/Down, Delete, F1-F12). It scrolls sideways.
// Modifiers are sticky: a tap holds one for the next key, from the bar or the phone's
// keyboard (a second tap lets it go). Other keys repeat while held.
import type { App } from './app';
import type { Keyboard } from './keyboard';

const REPEAT_AFTER_MS = 400;
const REPEAT_EVERY_MS = 60;
/** The visible area shrinks at least this much (CSS px) while a keyboard is up. */
const KEYBOARD_MIN_PX = 100;

interface Key {
	label: (mac: boolean) => string;
	ks: number;
	modifier?: boolean;
}

const MODIFIERS: Record<'ctrl' | 'alt' | 'super' | 'shift', Key> = {
	ctrl: {
		label: (mac) => (mac ? '⌃ Ctrl' : 'Ctrl'),
		ks: 0xffe3,
		modifier: true,
	},
	alt: { label: (mac) => (mac ? '⌥ Opt' : 'Alt'), ks: 0xffe9, modifier: true },
	super: {
		label: (mac) => (mac ? '⌘ Cmd' : 'Win'),
		ks: 0xffeb,
		modifier: true,
	},
	shift: {
		label: (mac) => (mac ? '⇧ Shift' : 'Shift'),
		ks: 0xffe1,
		modifier: true,
	},
};

const plain = (label: string, ks: number): Key => ({ label: () => label, ks });

/** The bar's keys; a Mac desktop gets Cmd first, since it is its shortcut key. */
function keys(mac: boolean): Key[] {
	const m = MODIFIERS;
	return [
		plain('Esc', 0xff1b),
		plain('Tab', 0xff09),
		...(mac ? [m.super, m.ctrl, m.alt] : [m.ctrl, m.alt, m.super]),
		m.shift,
		plain('←', 0xff51),
		plain('↓', 0xff54),
		plain('↑', 0xff52),
		plain('→', 0xff53),
		plain('Home', 0xff50),
		plain('End', 0xff57),
		plain('PgUp', 0xff55),
		plain('PgDn', 0xff56),
		plain('Del', 0xffff),
		...Array.from({ length: 12 }, (_, i) => plain(`F${i + 1}`, 0xffbe + i)),
	];
}

export function createExtraKeys(app: App, keyboard: Keyboard) {
	const { kbd, tools } = app;
	const bar = document.createElement('div');
	bar.id = 'extra-keys';
	bar.setAttribute('role', 'toolbar');
	bar.setAttribute('aria-label', 'Extra keys');
	bar.hidden = true;
	document.body.appendChild(bar);
	let builtFor: boolean | null = null;

	function build() {
		builtFor = app.remoteMac;
		bar.replaceChildren(
			...keys(app.remoteMac).map((key) => {
				const b = document.createElement('button');
				b.type = 'button';
				b.tabIndex = -1;
				b.textContent = key.label(app.remoteMac);
				b.dataset.ks = String(key.ks);
				if (key.modifier) b.dataset.modifier = '1';
				return b;
			})
		);
		showLatched();
	}
	function showLatched() {
		for (const b of bar.querySelectorAll<HTMLElement>('[data-modifier]'))
			b.classList.toggle('on', keyboard.isLatched(Number(b.dataset.ks)));
	}
	keyboard.onLatch(showLatched);

	/** Sits right on top of the on-screen keyboard, where the visual viewport ends (on
	 * Android the page shrinks above the keyboard: interactive-widget in index.html). */
	function place() {
		const vv = window.visualViewport;
		const bottom = vv ? vv.offsetTop + vv.height : window.innerHeight;
		bar.style.top = `${bottom - bar.offsetHeight}px`;
	}

	// The desktop keeps the size and place it had before the keyboard came up, and the
	// keyboard covers its lower part, as on iOS: the page shrinking on Android would
	// otherwise shrink the desktop and, with dynamic resolution, resize it on the server.
	let full = { w: window.innerWidth, h: window.innerHeight };
	const keyboardUp = () =>
		tools.classList.contains('touch') &&
		!!kbd.dataset.soft &&
		document.activeElement === kbd;
	app.layoutHeight = () => {
		if (
			keyboardUp() &&
			window.innerWidth === full.w &&
			window.innerHeight < full.h
		)
			return full.h;
		full = { w: window.innerWidth, h: window.innerHeight };
		return full.h;
	};
	function update() {
		const height = app.layoutHeight();
		const visible = window.visualViewport?.height ?? window.innerHeight;
		// Android's Back button hides the keyboard but leaves the focus where it was.
		const on =
			keyboardUp() && !app.readOnly && visible < height - KEYBOARD_MIN_PX;
		if (on && builtFor !== app.remoteMac) build();
		bar.hidden = !on;
		if (on) place();
		document.body.style.height =
			height > window.innerHeight ? `${height}px` : '';
	}
	kbd.addEventListener('focus', update);
	kbd.addEventListener('blur', update);
	window.visualViewport?.addEventListener('resize', update);
	window.visualViewport?.addEventListener('scroll', update);
	window.addEventListener('resize', update);

	let repeat: ReturnType<typeof setTimeout> | undefined;
	let repeated = false;
	const stop = () => {
		clearTimeout(repeat);
		repeat = undefined;
	};
	// Keeps the focus (and the on-screen keyboard) on the hidden textarea.
	bar.addEventListener('pointerdown', (e) => {
		e.preventDefault();
		e.stopPropagation();
		const b = (e.target as Element).closest<HTMLElement>('button');
		repeated = false;
		stop();
		if (!b || b.dataset.modifier) return;
		const ks = Number(b.dataset.ks);
		repeat = setTimeout(function again() {
			repeated = true;
			keyboard.tap(ks);
			repeat = setTimeout(again, REPEAT_EVERY_MS);
		}, REPEAT_AFTER_MS);
	});
	for (const type of ['pointerup', 'pointercancel', 'pointerleave'])
		bar.addEventListener(type, stop);
	bar.addEventListener('click', (e) => {
		const b = (e.target as Element).closest<HTMLElement>('button');
		if (document.activeElement !== kbd) kbd.focus({ preventScroll: true });
		if (!b || repeated) return;
		const ks = Number(b.dataset.ks);
		if (b.dataset.modifier) keyboard.toggleLatch(ks);
		else keyboard.tap(ks);
	});

	return { update };
}
