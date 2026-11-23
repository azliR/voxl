// Stats overlay (the toolbar's stats button, or ?stats in the URL): frame rate, bitrate,
// and where a frame's latency goes, from the screen changing on the server to the
// frame being decoded here. The server's stages come from its timing messages
// (src/timing.rs); one-way network time is half the round trip, since the two
// machines' clocks can't be compared.
import type { Stats, Timing } from '../protocol';
import type { App } from './app';
import { el } from './app';

/** localStorage key that remembers an open stats overlay on this device. */
const STATS_KEY = 'voxl-stats';

const ms = (v: number | null | undefined) =>
	v === null || v === undefined ? '  –  ' : v.toFixed(1).padStart(5);

export function createStats(app: App, forced: boolean) {
	const box = el('stats');
	const button = el<HTMLButtonElement>('stats-btn');
	let on = forced;
	try {
		on ||= localStorage.getItem(STATS_KEY) === '1';
	} catch {}
	let stats: Stats | null = null;
	let timing: Timing | null = null;

	function render() {
		button.classList.toggle('on', on);
		button.setAttribute('aria-pressed', String(on));
		box.style.display = on ? 'block' : 'none';
		if (!on) return;
		const moving = !!stats && stats.fps > 0;
		const server = moving ? stats!.pipeMs : null;
		const network = timing ? timing.rtt / 2 : null;
		const decode = moving ? stats!.decodeMs : null;
		const total =
			server !== null && network !== null && decode !== null
				? server + network + decode
				: null;
		const lines = [
			`${(stats?.fps ?? 0).toFixed(1)} fps   ${((stats?.kbps ?? 0) / 1000).toFixed(2)} Mbps   ${stats?.codec ?? ''}`,
			'',
			`server   ${ms(server)} ms`,
			`  wait     ${ms(timing?.wait)}`,
			`  grab     ${ms(timing?.grab)}`,
			`  convert  ${ms(timing?.convert)}`,
			`  encode   ${ms(timing?.encode)}`,
			`network  ${ms(network)} ms   (RTT ${ms(timing?.rtt).trim()}, min ${timing?.minRtt ?? '–'}, queue ${ms(timing?.queue).trim()})`,
			`decode   ${ms(decode)} ms`,
			`total   ≈${ms(total)} ms`,
			'',
			`stripes ${stats?.stripes ?? 0}   queued ${stats?.queue ?? 0}   dropped ${stats?.dropped ?? 0}`,
		];
		if (!moving) lines.push('(screen is still: no frames to time)');
		box.textContent = lines.join('\n');
	}

	button.addEventListener('click', () => {
		on = !on;
		try {
			if (on) localStorage.setItem(STATS_KEY, '1');
			else localStorage.removeItem(STATS_KEY);
		} catch {}
		render();
		if (!app.tools.classList.contains('touch'))
			app.kbd.focus({ preventScroll: true });
	});
	render();

	return {
		setStats(s: Stats) {
			stats = s;
			render();
		},
		setTiming(t: Timing) {
			timing = t;
			render();
		},
	};
}
