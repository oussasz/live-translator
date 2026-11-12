import { act, fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";
import type { PopupState } from "../lib/types";

let push: (p: PopupState) => void = () => {};
vi.mock("../lib/ipc", () => ({
  onPopup: (cb: (p: PopupState) => void) => {
    push = cb;
    return Promise.resolve(() => {});
  },
  cmd: { copyText: vi.fn(() => Promise.resolve()), popupReply: vi.fn(), popupClose: vi.fn() },
}));

import { cmd } from "../lib/ipc";
import ReadPopup from "./ReadPopup";

const base: PopupState = {
  visible: true,
  source: "La facture vous sera envoy\u00e9e.",
  translation: "The invoice will be sent.",
  detected: "fr",
  target: "en",
  error: null,
  pos: { x: 0, y: 0 },
};

function show(p: Partial<PopupState>) {
  render(<ReadPopup />);
  act(() => push({ ...base, ...p }));
}

test("shows_detected_and_translation", () => {
  show({});
  expect(screen.getByText("French detected \u2192 English")).toBeInTheDocument();
  expect(screen.getByText("The invoice will be sent.")).toBeInTheDocument();
});

test("copy_invokes_and_confirms", async () => {
  show({});
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));
  expect(cmd.copyText).toHaveBeenCalledWith("The invoice will be sent.");
  expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
});

test("reply_in_detected_language", () => {
  show({});
  fireEvent.click(screen.getByRole("button", { name: "Reply in French" }));
  expect(cmd.popupReply).toHaveBeenCalledWith("fr");
});

test("no_reply_when_same_language", () => {
  show({ detected: "en", target: "en" });
  expect(screen.queryByRole("button", { name: /Reply in/ })).toBeNull();
});

test("error_state", () => {
  show({ translation: null, error: "Offline" });
  expect(screen.getByText("Offline \u2014 will retry")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
});

test("close_button", () => {
  show({});
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(cmd.popupClose).toHaveBeenCalled();
});
