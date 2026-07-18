import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({
  plugins: [react()],
  base: "./",
  build: { outDir: "dist" },
  test: {
    environment: "jsdom",
    include: ["apps/desktop/src/**/*.test.ts", "apps/desktop/src/**/*.test.tsx"],
    setupFiles: ["apps/desktop/src/test/setup.ts"],
  },
});
