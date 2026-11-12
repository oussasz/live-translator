import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi, beforeEach } from "vitest";
import type { Settings } from "../../lib/types";

const defaults: Settings = {
  favorites: ["en", "fr", "ar"],
  target: "en",
  hotkeys: { toggle: "Ctrl+Alt+T", cycle_target: "Ctrl+Alt+L", read_selection: "Ctrl+Alt+R" },
  theme: "system",
  autostart: true,
  cap_enabled: true,
  provider: "groq",
  groq_model: "openai/gpt-oss-20b",
};

vi.mock("../../lib/ipc", () => ({
  onSettingsChanged: () => Promise.resolve(() => {}),
  cmd: {
    getSettings: vi.fn(),
    saveSettings: vi.fn(),
    hasApiKey: vi.fn(),
    setApiKey: vi.fn(),
    getUsage: vi.fn(),
  },
}));

import { cmd } from "../../lib/ipc";
import SettingsWindow from "./SettingsWindow";

const m = cmd as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeEach(() => {
  vi.clearAllMocks();
  m.getSettings.mockResolvedValue(structuredClone(defaults));
  m.saveSettings.mockResolvedValue(undefined);
  m.hasApiKey.mockResolvedValue({ google: true, groq: true });
  m.getUsage.mockResolvedValue({ month: "2026-11", chars: 41200, limit: 500000, requests_today: 120, request_limit: 1000 });
});

test("first_run_when_no_key", async () => {
  m.hasApiKey.mockResolvedValue({ google: false, groq: false });
  render(<SettingsWindow />);
  expect(await screen.findByRole("heading", { name: "Connect your translation service" })).toBeInTheDocument();
});

test("invalid_key_message", async () => {
  m.hasApiKey.mockResolvedValue({ google: false, groq: false });
  m.setApiKey.mockRejectedValue("InvalidKey");
  render(<SettingsWindow />);
  fireEvent.change(await screen.findByLabelText("Groq API key"), { target: { value: "bad" } });
  fireEvent.click(screen.getByRole("button", { name: "Test and save" }));
  expect(await screen.findByText("That key didn\u2019t work. Check it in Google Cloud Console.")).toBeInTheDocument();
});

test("first_run_success_then_languages_then_done", async () => {
  m.hasApiKey.mockResolvedValue({ google: false, groq: false });
  m.setApiKey.mockResolvedValue(undefined);
  render(<SettingsWindow />);
  fireEvent.change(await screen.findByLabelText("Groq API key"), { target: { value: "good" } });
  fireEvent.click(screen.getByRole("button", { name: "Test and save" }));
  expect(await screen.findByRole("heading", { name: "Pick your languages" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  await waitFor(() => expect(m.saveSettings).toHaveBeenCalled());
  expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
});

test("hotkey_recorder_rejects_duplicate", async () => {
  render(<SettingsWindow />);
  const rec = await screen.findByRole("button", { name: /Translate mode shortcut/ });
  fireEvent.click(rec);
  fireEvent.keyDown(rec, { key: "l", code: "KeyL", ctrlKey: true, altKey: true });
  expect(await screen.findByText("Already used for Cycle language")).toBeInTheDocument();
  expect(m.saveSettings).not.toHaveBeenCalled();
});

test("hotkey_recorder_saves_new_combo", async () => {
  render(<SettingsWindow />);
  const rec = await screen.findByRole("button", { name: /Translate mode shortcut/ });
  fireEvent.click(rec);
  fireEvent.keyDown(rec, { key: "y", code: "KeyY", ctrlKey: true, shiftKey: true });
  await waitFor(() =>
    expect(m.saveSettings).toHaveBeenCalledWith(expect.objectContaining({ hotkeys: expect.objectContaining({ toggle: "Ctrl+Shift+Y" }) })),
  );
});

test("theme_change_saves", async () => {
  render(<SettingsWindow />);
  fireEvent.click(await screen.findByRole("radio", { name: "Dark" }));
  await waitFor(() => expect(m.saveSettings).toHaveBeenCalledWith(expect.objectContaining({ theme: "dark" })));
});

test("first_run_saves_key_for_chosen_service", async () => {
  m.hasApiKey.mockResolvedValue({ google: false, groq: false });
  m.setApiKey.mockResolvedValue(undefined);
  render(<SettingsWindow />);
  fireEvent.change(await screen.findByLabelText("Groq API key"), { target: { value: "gsk_good" } });
  fireEvent.click(screen.getByRole("button", { name: "Test and save" }));
  await waitFor(() => expect(m.setApiKey).toHaveBeenCalledWith("groq", "gsk_good"));
});

test("switching_service_saves_provider_and_asks_for_missing_key", async () => {
  m.hasApiKey.mockResolvedValue({ google: false, groq: true });
  render(<SettingsWindow />);
  fireEvent.click(await screen.findByRole("radio", { name: "Google Translate" }));
  await waitFor(() => expect(m.saveSettings).toHaveBeenCalledWith(expect.objectContaining({ provider: "google" })));
  expect(await screen.findByLabelText("Google API key")).toBeInTheDocument();
});

test("model_select_saves", async () => {
  render(<SettingsWindow />);
  fireEvent.change(await screen.findByLabelText("Groq model"), { target: { value: "openai/gpt-oss-120b" } });
  await waitFor(() => expect(m.saveSettings).toHaveBeenCalledWith(expect.objectContaining({ groq_model: "openai/gpt-oss-120b" })));
});

test("groq_usage_shows_requests_today", async () => {
  m.getUsage.mockResolvedValue({ month: "2026-11", chars: 0, limit: 500000, requests_today: 850, request_limit: 1000 });
  render(<SettingsWindow />);
  expect(await screen.findByText("Today: 850 / 1,000 requests")).toBeInTheDocument();
  expect(screen.getByTestId("usage-bar")).toHaveAttribute("data-warn", "true");
});

test("usage_bar_formats_and_warns", async () => {
  m.getSettings.mockResolvedValue({ ...structuredClone(defaults), provider: "google" });
  m.getUsage.mockResolvedValue({ month: "2026-11", chars: 410000, limit: 500000, requests_today: 0, request_limit: 1000 });
  render(<SettingsWindow />);
  expect(await screen.findByText("This month: 410,000 / 500,000 free")).toBeInTheDocument();
  expect(screen.getByTestId("usage-bar")).toHaveAttribute("data-warn", "true");
});

test("save_error_is_shown", async () => {
  m.saveSettings.mockRejectedValue("Pick at least one language");
  render(<SettingsWindow />);
  fireEvent.click(await screen.findByRole("radio", { name: "Light" }));
  expect(await screen.findByText("Pick at least one language")).toBeInTheDocument();
});
