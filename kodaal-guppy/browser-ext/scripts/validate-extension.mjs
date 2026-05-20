import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const manifest = JSON.parse(fs.readFileSync(path.join(root, "manifest.json"), "utf8"));
const failures = [];

function requireFile(relativePath) {
  if (!fs.existsSync(path.join(root, relativePath))) {
    failures.push(`missing ${relativePath}`);
  }
}

for (const permission of manifest.host_permissions || []) {
  if (permission.includes("<all_urls>") || permission === "*://*/*") {
    failures.push(`overbroad host permission ${permission}`);
  }
  if (permission.startsWith("http://") && !permission.startsWith("http://127.0.0.1:7878/")) {
    failures.push(`non-local http permission ${permission}`);
  }
}

for (const script of manifest.content_scripts || []) {
  for (const file of script.js || []) {
    requireFile(file);
  }
}

requireFile(manifest.background.service_worker);
requireFile(manifest.action.default_popup);
for (const icon of Object.values(manifest.icons || {})) {
  requireFile(icon);
}
requireFile("sites.json");

const sites = JSON.parse(fs.readFileSync(path.join(root, "sites.json"), "utf8"));
for (const site of sites) {
  if (!site.name || !Array.isArray(site.matches) || !Array.isArray(site.input_selectors)) {
    failures.push(`invalid site entry ${site.name || "<unnamed>"}`);
  }
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log(`Browser extension validated: ${sites.length} supported sites`);
