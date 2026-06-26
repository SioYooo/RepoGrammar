#!/usr/bin/env node
"use strict";

const childProcess = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const https = require("node:https");
const os = require("node:os");
const path = require("node:path");

const packageJson = require("../../package.json");

function platformTarget(platform = process.platform, arch = process.arch) {
  const archMap = new Map([
    ["x64", "x86_64"],
    ["arm64", "aarch64"],
  ]);
  const normalizedArch = archMap.get(arch);
  if (!normalizedArch) {
    throw new Error(`unsupported architecture: ${arch}`);
  }
  if (platform === "darwin") {
    return `${normalizedArch}-apple-darwin`;
  }
  if (platform === "linux") {
    return `${normalizedArch}-unknown-linux-gnu`;
  }
  if (platform === "win32") {
    if (normalizedArch !== "x86_64") {
      throw new Error("Windows preview supports x86_64 only");
    }
    return "x86_64-pc-windows-msvc";
  }
  throw new Error(`unsupported platform: ${platform}`);
}

function defaultReleaseTag() {
  return process.env.REPOGRAMMAR_VERSION || `v${packageJson.version}`;
}

function artifactName(target, platform = process.platform) {
  if (platform === "win32") {
    return `repogrammar-${target}.zip`;
  }
  return `repogrammar-${target}.tar.gz`;
}

function releaseBase(tag = defaultReleaseTag()) {
  if (process.env.REPOGRAMMAR_RELEASE_BASE) {
    return process.env.REPOGRAMMAR_RELEASE_BASE.replace(/\/+$/, "");
  }
  const repo = process.env.REPOGRAMMAR_REPO || "SioYooo/RepoGrammar";
  return `https://github.com/${repo}/releases/download/${tag}`;
}

function cacheRoot() {
  return (
    process.env.REPOGRAMMAR_NPM_CACHE_DIR ||
    path.join(os.homedir(), ".repogrammar", "npm")
  );
}

function binaryName(platform = process.platform) {
  return platform === "win32" ? "repogrammar.exe" : "repogrammar";
}

function binaryPath(target, tag = defaultReleaseTag()) {
  return path.join(cacheRoot(), tag, target, binaryName());
}

function ensureDirectory(directory) {
  fs.mkdirSync(directory, { recursive: true });
}

function readLocalAsset(name, destination) {
  const releaseDir = process.env.REPOGRAMMAR_RELEASE_DIR;
  if (!releaseDir) {
    return false;
  }
  fs.copyFileSync(path.join(releaseDir, name), destination);
  return true;
}

function download(url, destination) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, (response) => {
      if (
        response.statusCode >= 300 &&
        response.statusCode < 400 &&
        response.headers.location
      ) {
        response.resume();
        download(response.headers.location, destination).then(resolve, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`download failed with HTTP ${response.statusCode}`));
        return;
      }
      const file = fs.createWriteStream(destination, { mode: 0o600 });
      response.pipe(file);
      file.on("finish", () => file.close(resolve));
      file.on("error", reject);
    });
    request.on("error", reject);
  });
}

async function fetchAsset(name, destination) {
  if (readLocalAsset(name, destination)) {
    return;
  }
  await download(`${releaseBase()}/${name}`, destination);
}

function sha256File(filePath) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
}

function verifyChecksum(archivePath, checksumPath) {
  const expected = fs
    .readFileSync(checksumPath, "utf8")
    .trim()
    .split(/\s+/)[0]
    .toLowerCase();
  const actual = sha256File(archivePath);
  if (!expected || expected !== actual) {
    throw new Error(`checksum verification failed for ${path.basename(archivePath)}`);
  }
}

function extractArchive(archivePath, destination, platform = process.platform) {
  ensureDirectory(destination);
  if (platform === "win32") {
    const command = [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Expand-Archive -LiteralPath ${JSON.stringify(
        archivePath
      )} -DestinationPath ${JSON.stringify(destination)} -Force`,
    ];
    childProcess.execFileSync("powershell", command, { stdio: "ignore" });
    return;
  }
  childProcess.execFileSync("tar", ["-xzf", archivePath, "-C", destination], {
    stdio: "ignore",
  });
}

function isInstalled(binary) {
  if (!fs.existsSync(binary)) {
    return false;
  }
  const worker = path.join(path.dirname(binary), "workers", "python", "worker.py");
  return fs.existsSync(worker);
}

async function ensureBinary() {
  const binaryOverride = process.env.REPOGRAMMAR_BINARY;
  if (binaryOverride && binaryOverride.trim()) {
    if (!path.isAbsolute(binaryOverride)) {
      throw new Error("REPOGRAMMAR_BINARY must be an absolute path");
    }
    const stat = fs.statSync(binaryOverride, { throwIfNoEntry: false });
    if (!stat || !stat.isFile()) {
      throw new Error("REPOGRAMMAR_BINARY must point to an existing file");
    }
    return binaryOverride;
  }
  const target = platformTarget();
  const tag = defaultReleaseTag();
  const installed = binaryPath(target, tag);
  if (isInstalled(installed)) {
    return installed;
  }
  const installDir = path.dirname(installed);
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-"));
  try {
    const artifact = artifactName(target);
    const archivePath = path.join(tempDir, artifact);
    const checksumPath = path.join(tempDir, `${artifact}.sha256`);
    await fetchAsset(artifact, archivePath);
    await fetchAsset(`${artifact}.sha256`, checksumPath);
    verifyChecksum(archivePath, checksumPath);
    extractArchive(archivePath, tempDir);
    const extractedBinary = path.join(tempDir, binaryName());
    if (!fs.existsSync(extractedBinary)) {
      throw new Error(`release artifact did not contain ${binaryName()}`);
    }
    fs.rmSync(installDir, { recursive: true, force: true });
    ensureDirectory(installDir);
    fs.copyFileSync(extractedBinary, installed);
    if (process.platform !== "win32") {
      fs.chmodSync(installed, 0o755);
    }
    const workerSource = path.join(tempDir, "workers", "python", "worker.py");
    if (fs.existsSync(workerSource)) {
      const workerDestination = path.join(installDir, "workers", "python");
      ensureDirectory(workerDestination);
      fs.copyFileSync(workerSource, path.join(workerDestination, "worker.py"));
    }
    return installed;
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

async function main(argv = process.argv.slice(2)) {
  const binary = await ensureBinary();
  const result = childProcess.spawnSync(binary, argv, { stdio: "inherit" });
  if (result.error) {
    throw result.error;
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
    return;
  }
  process.exit(result.status === null ? 1 : result.status);
}

if (require.main === module) {
  main().catch((error) => {
    console.error(`repogrammar npm launcher: ${error.message}`);
    process.exit(1);
  });
}

module.exports = {
  artifactName,
  binaryPath,
  defaultReleaseTag,
  ensureBinary,
  platformTarget,
  verifyChecksum,
};
