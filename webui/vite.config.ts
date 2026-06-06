import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: "out",
    emptyOutDir: true
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:50992",
      "/health": "http://127.0.0.1:50992"
    }
  }
});
