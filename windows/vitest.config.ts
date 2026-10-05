import { defineConfig } from "vitest/config";

// Unit tests for the island's pure TypeScript: the open/close FSM, the shared
// state, and the layout math. No Tauri, no DOM — run with `npm test`.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    // fsm.ts schedules timers on window.setTimeout; tests fake them anyway,
    // but the reference must exist outside a browser.
    setupFiles: ["./vitest.setup.ts"],
  },
});
