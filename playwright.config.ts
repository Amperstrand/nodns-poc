import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 60_000,
  retries: 2,
  use: {
    baseURL: "https://beta.nodns.shop",
    headless: true,
  },
  projects: [
    {
      name: "beta",
      testDir: "./tests",
      testMatch: "beta.spec.ts",
    },
  ],
});
