// Unit tests for the shared app state (src/core/state.ts).
// The singleton is re-imported per test via `vi.resetModules()` so no test
// sees another test's tasks or settings.

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { AgentTask } from "./state";

let State: import("./state").AppState;
let INTEGRATION_AGENTS: AgentTask[];
let TOGGLEABLE_INTEGRATION_IDS: string[];

const TASK = (id: string, name = "T"): AgentTask => ({
  id,
  name,
  color: "#000",
  state: "idle",
  stepIndex: 0,
  steps: [],
  source: "agent",
  isIntegration: false,
});

async function freshState() {
  vi.resetModules();
  const mod = await import("./state");
  State = mod.State;
  ({ INTEGRATION_AGENTS, TOGGLEABLE_INTEGRATION_IDS } = mod);
}

beforeEach(async () => {
  await freshState();
  State.tasks.length = 0;
});

describe("AppState", () => {
  describe("focus getters", () => {
    it("focusTask falls back to the first task", () => {
      State.tasks.push(TASK("a"));
      State.tasks.push(TASK("b"));
      expect(State.focusTask?.id).toBe("a");
    });

    it("focusTask picks the focused id", () => {
      State.tasks.push(TASK("a"), TASK("b"));
      State.focusId = "b";
      expect(State.focusTask?.id).toBe("b");
    });

    it("focusTask is null without tasks", () => {
      expect(State.focusTask).toBeNull();
    });

    it("effectiveState prefers the override", () => {
      State.tasks.push(TASK("a", "A"));
      State.stateOverride = "thinking";
      expect(State.effectiveState).toBe("thinking");
    });

    it("effectiveState falls back to the focus task, then idle", () => {
      expect(State.effectiveState).toBe("idle");
      const t = TASK("a");
      t.state = "working";
      State.tasks.push(t);
      expect(State.effectiveState).toBe("working");
    });

    it("otherTasks excludes the focused id", () => {
      State.tasks.push(TASK("a"), TASK("b"), TASK("c"));
      State.focusId = "b";
      expect(State.otherTasks.map((t) => t.id)).toEqual(["a", "c"]);
    });
  });

  describe("subscribe/notify", () => {
    it("notify calls every subscriber", () => {
      const a = vi.fn();
      const b = vi.fn();
      State.subscribe(a);
      State.subscribe(b);
      State.notify();
      expect(a).toHaveBeenCalledTimes(1);
      expect(b).toHaveBeenCalledTimes(1);
    });

    it("unsubscribe stops notifications", () => {
      const fn = vi.fn();
      const off = State.subscribe(fn);
      State.notify();
      off();
      State.notify();
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it("setFocus clears the pill badge and notifies", () => {
      State.tasks.push(TASK("integration_claude"));
      State.tasks[0].pillBadge = "error";
      const fn = vi.fn();
      State.subscribe(fn);
      State.setFocus("integration_claude");
      expect(State.tasks[0].pillBadge).toBeNull();
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it("setFocus ignores unknown ids", () => {
      const before = State.focusId;
      State.setFocus("nope");
      expect(State.focusId).toBe(before);
    });
  });

  describe("task mutations", () => {
    it("updateTask updates the state of a known task", () => {
      State.tasks.push(TASK("a"));
      State.updateTask("a", "working");
      expect(State.tasks[0].state).toBe("working");
    });

    it("appendStep keeps at most 20 steps", () => {
      State.tasks.push(TASK("a"));
      for (let i = 0; i < 25; i++) State.appendStep("a", `step ${i}`);
      const t = State.tasks[0];
      expect(t.steps.length).toBe(20);
      expect(t.steps[0]).toBe("step 5");
      expect(t.stepIndex).toBe(19);
    });

    it("setPillBadge sets the badge on the right task", () => {
      State.tasks.push(TASK("a"), TASK("b"));
      State.setPillBadge("b", "approval");
      expect(State.tasks[0].pillBadge ?? null).toBeNull();
      expect(State.tasks[1].pillBadge).toBe("approval");
    });

    it("removeTask reassigns focus when the focused task goes", () => {
      State.tasks.push(TASK("a"), TASK("b"));
      State.focusId = "a";
      State.removeTask("a");
      expect(State.focusId).toBe("b");
    });

    it("removeTask falls back to integration_claude when empty", () => {
      State.tasks.push(TASK("a"));
      State.focusId = "a";
      State.removeTask("a");
      expect(State.focusId).toBe("integration_claude");
    });

    it("upsertExternalAgent inserts after integration_claude", () => {
      State.tasks.push(TASK("integration_claude", "VS Code"), TASK("integration_stripe"));
      State.upsertExternalAgent("agent_x", "Codex", "#fff");
      expect(State.tasks.map((t) => t.id)).toEqual([
        "integration_claude",
        "agent_x",
        "integration_stripe",
      ]);
    });

    it("upsertExternalAgent no-ops for an existing id", () => {
      State.tasks.push(TASK("integration_claude"));
      State.upsertExternalAgent("agent_x", "Codex", "#fff");
      State.upsertExternalAgent("agent_x", "Codex", "#fff");
      expect(State.tasks.filter((t) => t.id === "agent_x").length).toBe(1);
    });

    it("upsertExternalAgent takes focus when nothing is focused", () => {
      State.tasks.push(TASK("integration_claude"));
      State.focusId = null;
      State.upsertExternalAgent("agent_x", "Codex", "#fff");
      expect(State.focusId).toBe("agent_x");
    });
  });

  describe("loadIntegrationTasks / toggleIntegration", () => {
    it("loads VS Code always, others opt-in", () => {
      State.settings.activeIntegrations = ["integration_stripe"];
      State.loadIntegrationTasks();
      expect(State.tasks.map((t) => t.id)).toEqual([
        "integration_claude",
        "integration_stripe",
      ]);
      expect(INTEGRATION_AGENTS.length).toBeGreaterThan(1);
      expect(TOGGLEABLE_INTEGRATION_IDS).not.toContain("integration_claude");
    });

    it("orders agent_ pills before other integrations", () => {
      State.tasks.push(TASK("integration_claude"), TASK("integration_stripe"));
      State.tasks.push(TASK("agent_codex"));
      State.settings.activeIntegrations = ["integration_stripe"];
      State.loadIntegrationTasks();
      expect(State.tasks.map((t) => t.id)).toEqual([
        "integration_claude",
        "agent_codex",
        "integration_stripe",
      ]);
    });

    it("toggleIntegration adds and removes", () => {
      State.settings.activeIntegrations = ["integration_stripe"];
      State.toggleIntegration("integration_stripe");
      expect(State.settings.activeIntegrations).toEqual([]);
      State.toggleIntegration("integration_stripe");
      expect(State.tasks.map((t) => t.id)).toEqual([
        "integration_claude",
        "integration_stripe",
      ]);
    });

    it("toggleIntegration caps at 4 active integrations", () => {
      State.settings.activeIntegrations = [
        "integration_resend",
        "integration_n8n",
        "integration_vercel",
        "integration_github",
      ];
      State.toggleIntegration("integration_notion");
      expect(State.settings.activeIntegrations.length).toBe(4);
      expect(State.settings.activeIntegrations).not.toContain("integration_notion");
    });

    it("toggleIntegration ignores VS Code", () => {
      const before = [...State.settings.activeIntegrations];
      State.toggleIntegration("integration_claude");
      expect(State.settings.activeIntegrations).toEqual(before);
    });
  });

  describe("defaultView", () => {
    it("empty without tasks, overview with tasks", () => {
      expect(State.defaultView()).toBe("empty");
      State.tasks.push(TASK("a"));
      expect(State.defaultView()).toBe("overview");
    });
  });
});
