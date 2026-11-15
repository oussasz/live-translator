import { act, render, screen, fireEvent } from "@testing-library/react";
import { vi } from "vitest";
import type { StripState } from "../lib/types";

let push: (s: StripState) => void = () => {};
vi.mock("../lib/ipc", () => ({
  onStrip: (cb: (s: StripState) => void) => {
    push = cb;
    return Promise.resolve(() => {});
  },
  cmd: { cycleTarget: vi.fn(), offlineChoice: vi.fn(), openSettings: vi.fn(), stripDragBy: vi.fn(), stripDragEnd: vi.fn(), stripUnpin: vi.fn(), overlayHover: vi.fn() },
}));

import { cmd } from "../lib/ipc";
import ComposeStrip from "./ComposeStrip";

const base: StripState = {
  visible: true,
  source: "",
  caret: 0,
  target: "en",
  detected: null,
  status: "idle",
  pos: { x: 0, y: 0 },
  offer_original: false,
};

function show(s: Partial<StripState>) {
  render(<ComposeStrip />);
  act(() => push({ ...base, ...s }));
}

test("renders_rtl_source_with_caret", () => {
  show({ source: "مرحبا", caret: 2 });
  const src = screen.getByTestId("source");
  expect(src).toHaveAttribute("dir", "auto");
  expect(src.textContent).toBe("مرحبا");
  expect(screen.getByTestId("before-caret").textContent).toBe("مر");
});

test("badge_shows_detected_to_target", () => {
  show({ detected: "ar", target: "en" });
  const badge = screen.getByRole("button", { name: /AR.*EN/ });
  fireEvent.click(badge);
  expect(cmd.cycleTarget).toHaveBeenCalled();
});

test("offline_offer_buttons", () => {
  show({ status: "offline", source: "salut", offer_original: true });
  fireEvent.click(screen.getByRole("button", { name: "Insert original" }));
  expect(cmd.offlineChoice).toHaveBeenCalledWith(true);
  fireEvent.click(screen.getByRole("button", { name: "Discard" }));
  expect(cmd.offlineChoice).toHaveBeenCalledWith(false);
});

test("key_error_opens_settings", () => {
  show({ status: "key_error" });
  expect(screen.getByText("API key problem")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
  expect(cmd.openSettings).toHaveBeenCalled();
});

test("status_messages", () => {
  show({ status: "password" });
  expect(screen.getByText("Not available in password fields")).toBeInTheDocument();
});

test("rate_limited_message", () => {
  show({ status: "rate_limited", source: "salut", caret: 5 });
  expect(screen.getByText("Groq rate limit \u2014 retrying")).toBeInTheDocument();
});

test("grip_drag_moves_and_pins", () => {
  show({ source: "abc", caret: 3 });
  const grip = screen.getByRole("button", { name: /Move the bar/ });
  fireEvent.pointerDown(grip, { screenX: 100, screenY: 100, pointerId: 1, button: 0 });
  fireEvent.pointerMove(grip, { screenX: 130, screenY: 90, pointerId: 1 });
  fireEvent.pointerMove(grip, { screenX: 135, screenY: 92, pointerId: 1 });
  fireEvent.pointerUp(grip, { screenX: 135, screenY: 92, pointerId: 1 });
  expect(cmd.stripDragBy).toHaveBeenNthCalledWith(1, 30, -10);
  expect(cmd.stripDragBy).toHaveBeenNthCalledWith(2, 5, 2);
  expect(cmd.stripDragEnd).toHaveBeenCalledTimes(1);
});

test("grip_double_click_follows_cursor_again", () => {
  show({ source: "abc", caret: 3 });
  fireEvent.doubleClick(screen.getByRole("button", { name: /Move the bar/ }));
  expect(cmd.stripUnpin).toHaveBeenCalled();
});

test("reports_pointer_over_strip", () => {
  show({ source: "abc", caret: 3 });
  const host = document.querySelector(".strip-host")!;
  fireEvent.mouseEnter(host);
  expect(cmd.overlayHover).toHaveBeenLastCalledWith(true);
  fireEvent.mouseLeave(host);
  expect(cmd.overlayHover).toHaveBeenLastCalledWith(false);
});

test("hidden_when_not_visible", () => {
  const { container } = render(<ComposeStrip />);
  act(() => push({ ...base, visible: false }));
  expect(container.querySelector(".strip")).toBeNull();
});
