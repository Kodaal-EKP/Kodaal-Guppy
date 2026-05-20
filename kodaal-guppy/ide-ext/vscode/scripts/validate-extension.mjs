import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const failures = [];

function requireFile(relativePath) {
  if (!fs.existsSync(path.join(root, relativePath))) {
    failures.push(`missing ${relativePath}`);
  }
}

requireFile(manifest.main);
for (const command of manifest.contributes.commands) {
  if (!command.command.startsWith("kodaal.")) {
    failures.push(`unexpected command id ${command.command}`);
  }
}

if (manifest.dependencies || manifest.devDependencies) {
  failures.push("extension must remain dependency-free until package manager gates exist");
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(`VS Code extension validated: ${manifest.contributes.commands.length} commands`);
