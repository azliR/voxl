// Pinch to zoom: magnifies the desktop on this screen (the remote desktop is
// unchanged). The view is a CSS transform on the surface, so pointer input, which
// maps client positions through the surface's on-screen box, follows it.
//
// A pinch zooms and moves the view together, and may move it past the desktop's edges
// (any edge can come to the middle of the window), to see around the on-screen keyboard
// or bring a corner within reach. Scrolling with two fingers moves the view only while
// the desktop is past the window's edges, then scrolls the desktop; it never pushes the
// view out farther.
import type { App } from './app';

/** Zoom in until one desktop pixel covers this many CSS pixels. */
const MAX_CSS_PX_PER_DESK_PX = 4;

export type Zoom = ReturnType<typeof createZoom>;

/** The offsets an axis allows (from `at`, the untransformed position) for the zoomed
 * desktop (`size` long) in a window `view` long. `snug`: covering the window, or
 * centered in it while it is smaller (unzoomed, the layout already centers it).
 * `loose`: up to an edge at the window's middle. */
function range(at: number, size: number, view: number, zoomed: boolean) {
	const snug: [number, number] = !zoomed
		? [0, 0]
		: size <= view
			? [(view - size) / 2 - at, (view - size) / 2 - at]
			: [view - size - at, -at];
	const loose: [number, number] = [view / 2 - size - at, view / 2 - at];
	return { snug, loose };
}

const clamp = (v: number, [lo, hi]: [number, number]) =>
	Math.min(hi, Math.max(lo, v));

/** Where an axis may go: anywhere `loose` allows, or (`snugOnly`) within `snug` and
 * no farther out than `current` already is. */
function place(
	t: number,
	current: number,
	at: number,
	size: number,
	view: number,
	zoomed: boolean,
	snugOnly: boolean
) {
	const { snug, loose } = range(at, size, view, zoomed);
	if (!snugOnly) return clamp(t, loose);
	return clamp(
		clamp(t, [Math.min(snug[0], current), Math.max(snug[1], current)]),
		loose
	);
}

export function createZoom(app: App) {
	const { surface } = app;
	let z = 1,
		tx = 0,
		ty = 0;
	/** The surface's untransformed box, in client pixels. */
	let box = { x: 0, y: 0, w: 0, h: 0 };
	/** The view and the fingers' centroid and spread when the pinch began. */
	let from = { z: 1, tx: 0, ty: 0, x: 0, y: 0, spread: 1 };

	function measure() {
		const r = surface.getBoundingClientRect();
		box = { x: r.left - tx, y: r.top - ty, w: r.width / z, h: r.height / z };
	}

	const limit = (nz: number) =>
		Math.min(
			Math.max(1, MAX_CSS_PX_PER_DESK_PX / app.view.scale),
			Math.max(1, nz)
		);

	function set(nz: number, ntx: number, nty: number, snugOnly: boolean) {
		if (!box.w) measure();
		z = limit(nz);
		// Trackpad mode's cursor scales back by this to keep its size (index.html).
		surface.style.setProperty('--zoom', String(z));
		const zoomed = z > 1;
		// Whole device pixels, so an unzoomed view moved aside stays sharp.
		const dpr = window.devicePixelRatio || 1;
		const snap = (v: number) => Math.round(v * dpr) / dpr;
		tx = snap(
			place(ntx, tx, box.x, box.w * z, window.innerWidth, zoomed, snugOnly)
		);
		ty = snap(
			place(nty, ty, box.y, box.h * z, window.innerHeight, zoomed, snugOnly)
		);
		surface.style.transform =
			z === 1 && !tx && !ty ? '' : `translate(${tx}px, ${ty}px) scale(${z})`;
	}

	return {
		/** Start a pinch with the fingers' centroid at (x, y), `spread` apart. */
		begin(x: number, y: number, spread: number) {
			measure();
			from = { z, tx, ty, x, y, spread: Math.max(1, spread) };
		},
		/** Zoom by how much the fingers spread and move with them, keeping the desktop
		 * under them; the view may go past the desktop's edges. */
		pinch(x: number, y: number, spread: number) {
			const nz = limit((from.z * Math.max(1, spread)) / from.spread);
			// The desktop point that was under the centroid stays under it.
			const ux = (from.x - box.x - from.tx) / from.z,
				uy = (from.y - box.y - from.ty) / from.z;
			set(nz, x - box.x - nz * ux, y - box.y - nz * uy, false);
		},
		/** Move the view by (dx, dy) while the desktop is past the window's edges;
		 * returns what it couldn't move (at an edge). */
		pan(dx: number, dy: number): [number, number] {
			const ox = tx,
				oy = ty;
			set(z, tx + dx, ty + dy, true);
			return [dx - (tx - ox), dy - (ty - oy)];
		},
		/** Keep the view in bounds after the desktop is laid out again. */
		fit() {
			measure();
			set(z, tx, ty, false);
		},
	};
}
