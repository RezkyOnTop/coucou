// fsm.ts and other island modules reference `window.setTimeout` /
// `window.clearTimeout`. Tests fake the timers, but the reference itself must
// exist in the node environment — a minimal stand-in is enough.
(globalThis as { window?: typeof globalThis }).window = globalThis;
