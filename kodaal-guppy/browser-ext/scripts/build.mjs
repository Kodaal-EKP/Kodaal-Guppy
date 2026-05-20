import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const repo = path.resolve(root, "..");
const manifest = JSON.parse(fs.readFileSync(path.join(root, "manifest.json"), "utf8"));
const version = manifest.version;
const targetArg = process.argv[2] || "all";
const targets = targetArg === "all" ? ["chromium", "firefox"] : [targetArg];

const files = [
  "background.js",
  "content.js",
  "popup.css",
  "popup.html",
  "popup.js",
  "sites.json",
  "icons/icon-16.png",
  "icons/icon-32.png",
  "icons/icon-48.png",
  "icons/icon-128.png"
];

function copyFile(relativePath, outDir) {
  const source = path.join(root, relativePath);
  const dest = path.join(outDir, relativePath);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(source, dest);
}

function buildTarget(target) {
  if (!["chromium", "firefox"].includes(target)) {
    throw new Error(`Unknown target ${target}`);
  }
  const outDir = path.join(repo, "dist", target);
  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });
  for (const file of files) {
    copyFile(file, outDir);
  }
  const targetManifest = { ...manifest };
  if (target === "firefox") {
    targetManifest.browser_specific_settings = {
      gecko: {
        id: "guppy@kodaal.local",
        strict_min_version: "121.0"
      }
    };
  }
  fs.writeFileSync(path.join(outDir, "manifest.json"), `${JSON.stringify(targetManifest, null, 2)}\n`);
  const archiveName = target === "firefox"
    ? `browser-ext-firefox-${version}.xpi`
    : `browser-ext-chromium-${version}.zip`;
  writeZip(outDir, path.join(repo, "dist", archiveName));
  console.log(`Built ${target} extension at ${outDir}`);
}

const crcTable = new Uint32Array(256).map((_, index) => {
  let c = index;
  for (let bit = 0; bit < 8; bit += 1) {
    c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  return c >>> 0;
});

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function dosTime(date) {
  return ((date.getHours() & 0x1f) << 11) | ((date.getMinutes() & 0x3f) << 5) | ((date.getSeconds() / 2) & 0x1f);
}

function dosDate(date) {
  return (((date.getFullYear() - 1980) & 0x7f) << 9) | (((date.getMonth() + 1) & 0x0f) << 5) | (date.getDate() & 0x1f);
}

function listFiles(dir, base = dir) {
  const entries = [];
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name);
    const stat = fs.statSync(full);
    if (stat.isDirectory()) {
      entries.push(...listFiles(full, base));
    } else if (stat.isFile()) {
      entries.push(path.relative(base, full).replace(/\\/g, "/"));
    }
  }
  return entries.sort();
}

function u16(value) {
  const buffer = Buffer.alloc(2);
  buffer.writeUInt16LE(value & 0xffff);
  return buffer;
}

function u32(value) {
  const buffer = Buffer.alloc(4);
  buffer.writeUInt32LE(value >>> 0);
  return buffer;
}

function writeZip(sourceDir, destFile) {
  const chunks = [];
  const central = [];
  let offset = 0;
  for (const relative of listFiles(sourceDir)) {
    const full = path.join(sourceDir, relative);
    const data = fs.readFileSync(full);
    const name = Buffer.from(relative);
    const stat = fs.statSync(full);
    const date = stat.mtime;
    const crc = crc32(data);
    const local = Buffer.concat([
      u32(0x04034b50),
      u16(20),
      u16(0),
      u16(0),
      u16(dosTime(date)),
      u16(dosDate(date)),
      u32(crc),
      u32(data.length),
      u32(data.length),
      u16(name.length),
      u16(0),
      name
    ]);
    chunks.push(local, data);
    central.push(Buffer.concat([
      u32(0x02014b50),
      u16(20),
      u16(20),
      u16(0),
      u16(0),
      u16(dosTime(date)),
      u16(dosDate(date)),
      u32(crc),
      u32(data.length),
      u32(data.length),
      u16(name.length),
      u16(0),
      u16(0),
      u16(0),
      u16(0),
      u32(0),
      u32(offset),
      name
    ]));
    offset += local.length + data.length;
  }
  const centralOffset = offset;
  const centralSize = central.reduce((sum, chunk) => sum + chunk.length, 0);
  const end = Buffer.concat([
    u32(0x06054b50),
    u16(0),
    u16(0),
    u16(central.length),
    u16(central.length),
    u32(centralSize),
    u32(centralOffset),
    u16(0)
  ]);
  fs.mkdirSync(path.dirname(destFile), { recursive: true });
  fs.writeFileSync(destFile, Buffer.concat([...chunks, ...central, end]));
}

for (const target of targets) {
  buildTarget(target);
}
