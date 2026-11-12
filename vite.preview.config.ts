// Dev-only gallery of the UI with a mocked Tauri bridge: pnpm vite --config vite.preview.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const mock = fileURLToPath(new URL("./src/preview/mockIpc.ts", import.meta.url));

export default defineConfig({
  plugins: [
    react(),
    {
      name: "mock-ipc",
      enforce: "pre",
      resolveId(source, importer) {
        if (importer && !importer.includes("preview") && /\/lib\/ipc$/.test(source)) return mock;
        return null;
      },
    },
  ],
  server: { port: 1430, strictPort: true },
});
