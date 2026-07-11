import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";

export default defineConfig({
  base: "/dashboard/",
  plugins: [vue()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    port: 30001,
    strictPort: true,
    host: "127.0.0.1",
    proxy: {
      "/dashboard/api": "http://127.0.0.1:9042",
    },
    watch: {
      ignored: ["**/target/**", "**/src-tauri/target/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2022",
    minify: true,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
