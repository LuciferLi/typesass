import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";
import autoImport from "unplugin-auto-import/vite";
import { defineConfig } from "vite";

export default defineConfig(() => {
  return {
    clearScreen: false,
    plugins: [
      vue(),
      autoImport({
        imports: ["vue", "vue-router"],
        dts: "src/autoImports.d.ts",
      }),
    ],
    resolve: {
      alias: {
        "@": fileURLToPath(new URL("./src", import.meta.url)),
      },
    },
    server: {
      host: "0.0.0.0",
      port: 1420,
      strictPort: true,
      proxy: {
        "/v1/local-images": {
          target: "http://127.0.0.1:18080",
          changeOrigin: true,
        },
      },
    },
  };
});
