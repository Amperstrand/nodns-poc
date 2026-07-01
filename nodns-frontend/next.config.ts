import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "export",
  basePath: process.env.GITHUB_PAGES === "1" ? "/nodns-poc" : undefined,
  images: {
    unoptimized: true,
  },
};

export default nextConfig;
