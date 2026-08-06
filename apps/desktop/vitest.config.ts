import { defineConfig } from "vitest/config";

/**
 * THE TEST RUN IS ITS OWN CONFIG, DELIBERATELY.
 *
 * `vite.config.ts` is what ships the app — a Tauri dev port, a build target, an
 * output directory — and a `test` block bolted onto it would put jsdom in the
 * same file as the release build. Vitest prefers this file when it is present,
 * so the two never have to agree about anything.
 *
 * No React plugin: esbuild already reads `jsx: "react-jsx"` out of
 * `tsconfig.json`, and fast refresh has nothing to refresh here.
 */
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.tsx"],
    restoreMocks: true,
  },
});
