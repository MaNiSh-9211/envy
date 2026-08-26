#!/usr/bin/env node
"use strict";

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const osMap = { win32: "windows", darwin: "darwin", linux: "linux" };
const cpuMap = { x64: "amd64", arm64: "arm64" };
const os = osMap[process.platform];
const cpu = cpuMap[process.arch];

if (!os || !cpu) {
  console.error(`envy: unsupported platform ${process.platform}-${process.arch}`);
  process.exit(1);
}

const exe = path.join(__dirname, "bin", `envy-${os}-${cpu}${os === "windows" ? ".exe" : ""}`);

if (!fs.existsSync(exe)) {
  console.error("envy binary is missing — reinstall with:");
  console.error("  npm rebuild -g envy-cli");
  process.exit(1);
}

const child = spawn(exe, process.argv.slice(2), { stdio: "inherit", windowsHide: false });

child.on("error", (err) => {
  console.error(`envy: failed to launch binary: ${err.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code === null || code === undefined ? 1 : code);
});
