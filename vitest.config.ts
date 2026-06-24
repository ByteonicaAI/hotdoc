import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte({ hot: false })],
  // Force svelte to resolve its client (browser) entry under vitest.
  // Without this, the `worker`/`default` condition in svelte's package.json
  // routes the import to the server build, where `mount` is not defined.
  resolve: {
    conditions: ["browser"],
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./vitest.setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,js,svelte}"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html", "lcov"],
      include: ["src/**/*.{ts,js,svelte}"],
      exclude: ["src/**/*.{test,spec}.{ts,js,svelte}", "src/main.ts", "src/vite-env.d.ts"],
      thresholds: { lines: 80, functions: 80 },
    },
  },
});
