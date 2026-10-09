/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// テストの日時表示を固定する（ワーカーに継承される）
process.env.TZ = "Asia/Tokyo";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
  },
});
