#!/usr/bin/env node
"use strict";

const https = require("https");
const fs = require("fs");
const path = require("path");
const { URL } = require("url");

const REPO = "MaNiSh-9211/envy";
const VERSION = process.env.ENVY_VERSION || "latest";
const BASE =
  VERSION === "latest"
    ? `https://github.com/${REPO}/releases/latest/download`
    : `https://github.com/${REPO}/releases/download/${VERSION}`;

function assetName() {
  const osMap = { win32: "windows", darwin: "darwin", linux: "linux" };
  const cpuMap = { x64: "amd64", arm64: "arm64" };
  const os = osMap[process.platform];
  const cpu = cpuMap[process.arch];
  if (!os || !cpu) {
    console.error(`envy: unsupported platform ${process.platform}-${process.arch}`);
    process.exit(1);
  }
  return `envy-${os}-${cpu}${os === "windows" ? ".exe" : ""}`;
}

function fetchBuffer(url, redirects) {
  return new Promise((resolve, reject) => {
    if (redirects > 5) {
      reject(new Error("too many redirects"));
      return;
    }
    https
      .get(url, { headers: { "user-agent": "envy-installer" } }, (res) => {
        const status = res.statusCode || 0;
        if (status >= 300 && status < 400 && res.headers.location) {
          res.resume();
          resolve(fetchBuffer(new URL(res.headers.location, url).href, redirects + 1));
          return;
        }
        if (status !== 200) {
          res.resume();
          reject(new Error(`HTTP ${status} for ${url}`));
          return;
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

async function main() {
  const asset = assetName();
  const url = `${BASE}/${asset}`;
  process.stdout.write(`envy: downloading ${asset} ...`);
  const buffer = await fetchBuffer(url, 0);
  const outDir = path.join(__dirname, "bin");
  fs.mkdirSync(outDir, { recursive: true });
  const outPath = path.join(outDir, asset);
  fs.writeFileSync(outPath, buffer);
  if (process.platform !== "win32") {
    fs.chmodSync(outPath, 0o755);
  }
  console.log(" done");
}

main().catch((err) => {
  console.error(`envy install failed: ${err.message}`);
  console.error(`checked: ${BASE}/${assetName()}`);
  process.exit(1);
});
