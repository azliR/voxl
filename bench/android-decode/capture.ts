// Records the H.264 a voxl tab in Chrome on an Android phone decodes, with Chrome's own
// decode latency for each frame, so the native benchmark replays the exact same frames.
// Usage: adb forward tcp:9333 localabstract:chrome_devtools_remote
//        bun capture.ts <page id from localhost:9333/json/list> <seconds> <out.h264>
import { writeFileSync } from 'node:fs';

const [pageId, secondsArg, outPath] = process.argv.slice(2);
if (!pageId || !outPath) throw new Error('usage: bun capture.ts <page id> <seconds> <out.h264>');
const seconds = Number(secondsArg);

const socket = new WebSocket(`ws://localhost:9333/devtools/page/${pageId}`);
let nextId = 1;
const pending = new Map<number, (value: any) => void>();
const workerSessions: string[] = [];
const patchedSessions = new Set<string>();

function send(method: string, params: object = {}, sessionId?: string): Promise<any> {
	const id = nextId++;
	socket.send(JSON.stringify({ id, method, params, sessionId }));
	return new Promise((resolve) => pending.set(id, resolve));
}

const recorder = `(() => {
  const record = self.__capture = { frames: [], latency: [] };
  const Original = self.VideoDecoder;
  self.VideoDecoder = class extends Original {
    constructor(init) {
      const submitted = new Map();
      super({ ...init, output: (frame) => {
        const entry = submitted.get(frame.timestamp);
        if (entry) { record.latency[entry.index] = performance.now() - entry.time; submitted.delete(frame.timestamp); }
        init.output(frame);
      }});
      this.__submitted = submitted;
    }
    decode(chunk) {
      const bytes = new Uint8Array(chunk.byteLength);
      chunk.copyTo(bytes);
      const index = record.frames.length;
      record.frames.push(bytes);
      this.__submitted.set(chunk.timestamp, { index, time: performance.now() });
      return super.decode(chunk);
    }
  };
})()`;

function readSlice(start: number, count: number) {
	return `(() => {
    const record = self.__capture;
    const out = [];
    for (const bytes of record.frames.slice(${start}, ${start + count})) {
      let text = '';
      for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      out.push(btoa(text));
    }
    return JSON.stringify({ total: record.frames.length, frames: out, latency: record.latency.slice(${start}, ${start + count}) });
  })()`;
}

socket.onmessage = async (event) => {
	const message = JSON.parse(String(event.data));
	if (message.id && pending.has(message.id)) {
		pending.get(message.id)!(message.result ?? message.error);
		pending.delete(message.id);
		return;
	}
	if (message.method === 'Debugger.paused' && message.sessionId) {
		const sessionId = message.sessionId;
		if (!patchedSessions.has(sessionId)) {
			patchedSessions.add(sessionId);
			await send('Runtime.evaluate', { expression: recorder }, sessionId);
			await send('Debugger.disable', {}, sessionId);
		}
		await send('Debugger.resume', {}, sessionId);
		return;
	}
	if (message.method !== 'Target.attachedToTarget') return;
	const sessionId = message.params.sessionId;
	if (message.params.targetInfo.type === 'worker') {
		workerSessions.push(sessionId);
		await send('Debugger.enable', {}, sessionId);
		await send(
			'Debugger.setInstrumentationBreakpoint',
			{ instrumentation: 'beforeScriptExecution' },
			sessionId
		);
	}
	await send('Runtime.runIfWaitingForDebugger', {}, sessionId);
};

socket.onopen = async () => {
	await send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: true });
	await send('Page.reload', { ignoreCache: true });
	await Bun.sleep(seconds * 1000);
	const sessionId = workerSessions.at(-1);
	if (!sessionId || !patchedSessions.has(sessionId)) throw new Error('voxl worker was not instrumented');

	const accessUnitDelimiter = Uint8Array.of(0, 0, 0, 1, 9, 0xf0);
	const parts: Uint8Array[] = [];
	const latencies: string[] = [];
	for (let start = 0; ; start += 30) {
		const response = await send(
			'Runtime.evaluate',
			{ expression: readSlice(start, 30), returnByValue: true },
			sessionId
		);
		const slice = JSON.parse(response.result.value);
		for (const [index, encoded] of slice.frames.entries()) {
			parts.push(accessUnitDelimiter, Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0)));
			const latency = slice.latency[index];
			latencies.push(latency === null || latency === undefined ? '' : latency.toFixed(2));
		}
		if (start + 30 >= slice.total) break;
	}
	writeFileSync(outPath, Buffer.concat(parts));
	writeFileSync(outPath.replace(/\.h264$/, '') + '.chrome.csv', 'chrome_ms\n' + latencies.join('\n') + '\n');
	console.log(`${latencies.length} frames -> ${outPath}`);
	process.exit(0);
};
