import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig(async () => ({

  // The renderer is also loaded from Electron with a file:// URL.
  base: "./",

  clearScreen: false,
  server: {
    port: 5173,
    watch: {
      ignored: ["**/rust-backend/**"],
    },
  },
}));
