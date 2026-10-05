// Unit tests for the island open/close FSM (src/island/fsm.ts).
// It is pure logic — no DOM, no Tauri — so every transition and timer is
// testable headlessly. These tests document the ported behaviour of
// IslandStateMachine.swift.

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { IslandStateMachine } from "./fsm";
import type { FsmState } from "./fsm";

/** Machine with instant timers, so tests never actually wait. */
function machine() {
  const m = new IslandStateMachine();
  m.homeToPetitDelay = 0;
  m.petitToHiddenDelay = 0;
  m.greetAutoCollapseDelay = 0;
  m.greetHoverCollapseDelay = 0;
  return m;
}

/** Recorded (from, to) pairs of every transition. */
function recorder(m: IslandStateMachine): FsmState[] {
  const seen: FsmState[] = [];
  m.onTransition = (_from, to) => seen.push(to);
  return seen;
}

/** Advances fake timers until every pending timer has fired once. */
function flush() {
  vi.advanceTimersByTime(1000);
}

describe("IslandStateMachine", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  describe("mouseEntered", () => {
    it("hidden → petit on first hover", () => {
      const m = machine();
      const seen = recorder(m);
      m.mouseEntered();
      expect(m.state).toBe("petit");
      expect(seen).toEqual(["petit"]);
    });

    it("petit stays petit and cancels the pending hide", () => {
      const m = machine();
      m.mouseEntered(); // → petit
      m.mouseLeft(); // schedules petitHide
      m.mouseEntered(); // cancels it
      flush();
      expect(m.state).toBe("petit");
    });

    it("home stays home and cancels the pending collapse", () => {
      const m = machine();
      m.mouseEntered();
      m.click(); // → home
      m.mouseLeft(); // schedules homeCollapse
      m.mouseEntered(); // cancels it
      flush();
      expect(m.state).toBe("home");
    });

    it("coucou schedules the long hover collapse", () => {
      const m = machine();
      m.launch(); // → coucou
      m.greetComplete(); // short timer armed
      m.mouseEntered(); // must swap to the hover timer
      m.greetComplete(); // must NOT override the running hover timer
      flush();
      expect(m.state).toBe("petit");
    });
  });

  describe("mouseLeft", () => {
    it("petit schedules the hide timer", () => {
      const m = machine();
      m.mouseEntered();
      m.mouseLeft();
      flush();
      expect(m.state).toBe("hidden");
    });

    it("home schedules the collapse timer", () => {
      const m = machine();
      m.mouseEntered();
      m.click();
      m.mouseLeft();
      flush();
      expect(m.state).toBe("petit");
    });

    it("a pinned home never collapses on its own", () => {
      const m = machine();
      m.pinned = true;
      m.mouseEntered();
      m.click();
      m.mouseLeft();
      flush();
      expect(m.state).toBe("home");
    });

    it("coucou collapses to petit immediately", () => {
      const m = machine();
      m.launch();
      m.mouseLeft();
      expect(m.state).toBe("petit");
    });

    it("hidden stays hidden", () => {
      const m = machine();
      m.mouseLeft();
      expect(m.state).toBe("hidden");
    });
  });

  describe("click", () => {
    it("petit → home", () => {
      const m = machine();
      m.mouseEntered();
      m.click();
      expect(m.state).toBe("home");
    });

    it("is ignored in every other state", () => {
      for (const via of [
        () => {},
        (m: IslandStateMachine) => m.launch(),
      ] as const) {
        const m = machine();
        via(m);
        const before = m.state;
        m.click();
        expect(m.state).toBe(before);
        expect(m.state).not.toBe("home");
      }
    });
  });

  describe("greetComplete", () => {
    it("schedules the auto-collapse when no hover timer runs", () => {
      const m = machine();
      m.launch();
      m.greetComplete();
      flush();
      expect(m.state).toBe("petit");
    });

    it("is ignored outside coucou", () => {
      const m = machine();
      m.mouseEntered(); // petit
      m.greetComplete();
      flush();
      expect(m.state).toBe("petit");
    });
  });

  describe("reveal / forceHome / forcePetit / forceHidden", () => {
    it("reveal shows the compact form from hidden, then hides again", () => {
      const m = machine();
      m.reveal();
      expect(m.state).toBe("petit");
      flush();
      expect(m.state).toBe("hidden");
    });

    it("reveal is ignored when not hidden", () => {
      const m = machine();
      m.forceHome();
      m.reveal();
      expect(m.state).toBe("home");
    });

    it("forceHome opens expanded and cancels timers", () => {
      const m = machine();
      m.mouseLeft(); // hidden, would schedule nothing
      m.forceHome();
      m.mouseLeft(); // schedule collapse…
      m.forceHome(); // …then cancel it
      flush();
      expect(m.state).toBe("home");
    });

    it("forcePetit collapses and cancels timers", () => {
      const m = machine();
      m.forceHome();
      m.forcePetit();
      expect(m.state).toBe("petit");
    });

    it("forceHidden hides from anywhere and cancels timers", () => {
      const m = machine();
      m.forceHome();
      m.forceHidden();
      expect(m.state).toBe("hidden");
      flush();
      expect(m.state).toBe("hidden");
    });
  });

  describe("timers", () => {
    it("leave-enter-leave re-arms the petit hide each time", () => {
      const m = machine();
      m.mouseEntered();
      m.mouseLeft();
      vi.advanceTimersByTime(500);
      m.mouseEntered(); // cancel
      expect(m.state).toBe("petit");
      m.mouseLeft(); // re-arm
      flush();
      expect(m.state).toBe("hidden");
    });

    it("launch cancels any pending timer", () => {
      const m = machine();
      m.mouseEntered();
      m.mouseLeft(); // petitHide armed
      m.launch();
      flush();
      expect(m.state).toBe("coucou");
    });

    it("cancelTimers stops every pending transition", () => {
      const m = machine();
      m.forceHome();
      m.mouseLeft(); // homeCollapse armed
      m.cancelTimers();
      flush();
      expect(m.state).toBe("home");
    });
  });

  describe("transition guard", () => {
    it("never fires onTransition for a no-op transition", () => {
      const m = machine();
      const seen = recorder(m);
      m.mouseLeft(); // hidden → hidden, no event
      expect(seen).toEqual([]);
    });
  });
});
