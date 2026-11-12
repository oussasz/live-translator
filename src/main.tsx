import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "@fontsource/inter/400.css";
import "@fontsource/inter/600.css";
import "@fontsource/ibm-plex-sans-arabic/400.css";
import "@fontsource/ibm-plex-sans-arabic/600.css";
import "./styles/tokens.css";
import ComposeStrip from "./windows/ComposeStrip";
import ReadPopup from "./windows/ReadPopup";
import { ThemedRoot } from "./windows/ThemedRoot";

const label = getCurrentWebviewWindow().label;

function Placeholder() {
  return null;
}

const views: Record<string, React.ComponentType> = {
  strip: ComposeStrip,
  popup: ReadPopup,
};
const View = views[label] ?? Placeholder;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemedRoot>
      <View />
    </ThemedRoot>
  </React.StrictMode>,
);
