import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  timeout: 60_000,
  retries: 2,
  use: {
    baseURL: "https://amperstrand.github.io/nodns-poc/",
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
    {
      name: "search",
      testDir: "./tests",
      testMatch: "search.spec.ts",
    },
    {
      name: "profile",
      testDir: "./tests",
      testMatch: "profile.spec.ts",
    },
    {
      name: "content",
      testDir: "./tests",
      testMatch: "content.spec.ts",
    },
    {
      name: "registration",
      testDir: "./tests",
      testMatch: "registration-flow.spec.ts",
    },
    {
      name: "visual-qa",
      testDir: "./tests",
      testMatch: "visual-qa.spec.ts",
      timeout: 90_000,
    },
    {
      name: "accessibility",
      testDir: "./tests",
      testMatch: "accessibility.spec.ts",
    },
    {
      name: "navigation",
      testDir: "./tests",
      testMatch: "navigation.spec.ts",
    },
  ],
});
