import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // authenticate-pam ships a native .node addon - it must be loaded via
  // Node's normal require() at runtime, not bundled by Turbopack/webpack.
  serverExternalPackages: ["authenticate-pam"],
};

export default nextConfig;
