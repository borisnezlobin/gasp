import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTypescript from "eslint-config-next/typescript";

export default defineConfig([
  ...nextVitals,
  ...nextTypescript,
  { rules: { complexity: ["error", 15] } },
  globalIgnores([".next/**", "out/**", "build/**", "next-env.d.ts"]),
]);
