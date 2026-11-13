import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

const backend = process.env.VENGEANCE_BACKEND ?? "http://localhost:8080";

export default defineConfig({
  base: "/",
  plugins: [vue(), tailwindcss()],
  server: {
    proxy: {
      "/api": { target: backend, changeOrigin: false },
      "/auth": { target: backend, changeOrigin: false },
      "/healthz": { target: backend, changeOrigin: false },
    },
  },
  build: {
    outDir: "dist",
    // Inline nothing as data: URIs so a strict CSP (no data: in img/font-src) holds.
    assetsInlineLimit: 0,
  },
});
