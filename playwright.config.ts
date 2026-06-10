import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 60_000,
  retries: 2,
  use: {
    baseURL: "https://amperstrand.github.io/nodns-poc",
    headless: true,
  },
  projects: [
    {
      name: "pages",
      testDir: "./tests",
      testMatch: "pages.spec.ts",
    },
    {
      name: "api",
      testDir: "./tests",
      testMatch: "api.spec.ts",
    },
  ],
});
