// Runs every src/**/*.test.ts through Vite's module runner (no test framework needed).
import { readdirSync } from "node:fs";
import { runnerImport } from "vite";

const files = readdirSync("src", { recursive: true })
  .map(String)
  .filter((f) => f.endsWith(".test.ts"))
  .map((f) => `./src/${f.replaceAll("\\", "/")}`);
for (const f of files) await runnerImport(f);
console.log(`${files.length} test file(s) passed`);
