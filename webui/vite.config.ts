import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const apiTarget = process.env.VITE_LOGAGENT_API_TARGET || "http://127.0.0.1:50993";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "out",
    emptyOutDir: true
  },
  server: {
    proxy: {
      "/api": apiTarget,
      "/health": apiTarget
    }
  }
});
