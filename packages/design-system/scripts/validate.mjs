import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const stylesheet = await readFile(
  fileURLToPath(new URL("../tokens.css", import.meta.url)),
  "utf8",
);

const requiredVariables = [
  "--hawk-night",
  "--hawk-cream",
  "--hawk-lime",
  "--hawk-border",
  "--hawk-focus",
];

const missing = requiredVariables.filter((variable) => !stylesheet.includes(variable));
if (missing.length > 0) {
  throw new Error(`Design system variables missing: ${missing.join(", ")}`);
}

process.stdout.write("HAWK design system validated.\n");
