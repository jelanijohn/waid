import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const port = Number(process.env.WAID_DEV_PORT) || 1420;

// WebKitGTK (the Tauri webview on Linux) speculatively re-requests a page's
// previously seen subresources in parallel at navigation. Against a freshly
// started dev server that can deliver `Foo.svelte?svelte&type=style&lang.css`
// before `Foo.svelte` itself has been compiled. vite-plugin-svelte's load hook
// then misses its CSS cache and returns nothing, Vite falls back to the raw
// `.svelte` source, and Tailwind rejects it as CSS ("Invalid declaration:
// `onDestroy`"). Compiling the parent component first makes the cache hit.
// Dev-only; `pre` so it runs ahead of vite-plugin-svelte's own `pre` load hook.
/** @returns {import('vite').Plugin} */
function svelteStyleWarmup() {
  /** @type {import('vite').ViteDevServer | undefined} */
  let server;
  return {
    name: "waid:svelte-style-warmup",
    apply: "serve",
    enforce: "pre",
    /** @param {import('vite').ViteDevServer} s */
    configureServer(s) {
      server = s;
    },
    /** @param {string} id */
    async load(id) {
      if (!server) return;
      const q = id.indexOf("?");
      if (q === -1 || !id.slice(q).includes("type=style")) return;
      const file = id.slice(0, q);
      if (!file.endsWith(".svelte")) return;
      if (server.moduleGraph.getModuleById(file)?.transformResult) return;
      const root = server.config.root;
      const url = file.startsWith(root) ? file.slice(root.length) : file;
      // Dedupes with an in-flight transform of the same module; errors surface
      // on the component's own request, so they're ignored here.
      await server.transformRequest(url).catch(() => {});
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [svelteStyleWarmup(), tailwindcss(), sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
