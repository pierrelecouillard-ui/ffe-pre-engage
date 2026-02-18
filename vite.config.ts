import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  base: "/ffe-pre-engage/",
  plugins: [
    react(),
    VitePWA({
      registerType: "autoUpdate",
      injectRegister: "auto",
      includeAssets: ["favicon.ico", "apple-touch-icon.png"],
      manifest: {
        name: "FFE Pre-Engage",
        short_name: "FFE",
        start_url: "./",
        scope: "./",
        display: "standalone",
        background_color: "#ffffff",
        theme_color: "#ffffff",
        icons: [
          { src: "pwa-192.png", sizes: "192x192", type: "image/png" },
          { src: "pwa-512.png", sizes: "512x512", type: "image/png" },
          { src: "pwa-512.png", sizes: "512x512", type: "image/png", purpose: "any maskable" }
        ]
      }
    })
  ],
  clearScreen: false,
  server: { strictPort: true, port: 5173 },
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        alarm: "alarm.html"
      }
    }
  }
});
