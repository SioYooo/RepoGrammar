#!/usr/bin/env node
"use strict";

const childProcess = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const https = require("node:https");
const os = require("node:os");
const path = require("node:path");

const packageJson = require("../../package.json");

function detectLinuxLibc(report = process.report?.getReport?.()) {
  if (report?.header?.glibcVersionRuntime) {
    return "glibc";
  }
  const sharedObjects = Array.isArray(report?.sharedObjects) ? report.sharedObjects : [];
  if (sharedObjects.some((entry) => /(?:^|\/)ld-musl-[^/]+\.so(?:\.\d+)*$/i.test(entry))) {
    return "musl";
  }
  return "unknown";
}

function detectLinuxGlibcVersion(report = process.report?.getReport?.()) {
  const version = report?.header?.glibcVersionRuntime;
  return typeof version === "string" && /^\d+\.\d+(?:\.\d+)?$/.test(version)
    ? version
    : null;
}

function versionAtLeast(actual, minimum) {
  if (!/^\d+(?:\.\d+)*$/.test(actual || "") || !/^\d+(?:\.\d+)*$/.test(minimum || "")) {
    return false;
  }
  const actualParts = actual.split(".").map(Number);
  const minimumParts = minimum.split(".").map(Number);
  const width = Math.max(actualParts.length, minimumParts.length);
  for (let index = 0; index < width; index += 1) {
    const actualPart = actualParts[index] || 0;
    const minimumPart = minimumParts[index] || 0;
    if (actualPart !== minimumPart) {
      return actualPart > minimumPart;
    }
  }
  return true;
}

function platformTarget(
  platform = process.platform,
  arch = process.arch,
  linuxLibc = platform === "linux" ? detectLinuxLibc() : null,
  glibcVersion = platform === "linux" ? detectLinuxGlibcVersion() : null
) {
  if (platform === "win32") {
    throw new Error(
      "unsupported platform: win32; RepoGrammar release binaries support macOS and Linux only"
    );
  }
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
    if (linuxLibc !== "glibc") {
      const classification = linuxLibc === "musl" ? "musl" : "unknown libc";
      throw new Error(
        `unsupported Linux runtime: ${classification}; the supported release runtime requires glibc`
      );
    }
    const minimum = arch === "x64" ? "2.35" : "2.39";
    if (!versionAtLeast(glibcVersion, minimum)) {
      const detected = glibcVersion || "unknown";
      throw new Error(
        `unsupported Linux glibc ${detected}; ${arch} release binaries require glibc ${minimum}+`
      );
    }
    return `${normalizedArch}-unknown-linux-gnu`;
  }
  throw new Error(`unsupported platform: ${platform}`);
}

function defaultReleaseTag() {
  return validateReleaseTag(process.env.REPOGRAMMAR_VERSION || `v${packageJson.version}`);
}

function artifactName(target) {
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

function binaryName() {
  return "repogrammar";
}

function binaryPath(target, tag = defaultReleaseTag()) {
  const root = path.resolve(cacheRoot());
  const safeTag = validateReleaseTag(tag);
  const safeTarget = validatePathSegment(target, "release target");
  const candidate = path.join(root, safeTag, safeTarget, binaryName());
  assertPathInside(root, candidate);
  return candidate;
}

function validateReleaseTag(tag) {
  const value = String(tag || "").trim();
  if (!/^(?!.*\.\.)[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value)) {
    throw new Error("invalid RepoGrammar release tag");
  }
  return value;
}

function validatePathSegment(value, label) {
  const segment = String(value || "").trim();
  if (!/^(?!.*\.\.)[A-Za-z0-9][A-Za-z0-9._-]*$/.test(segment)) {
    throw new Error(`invalid ${label}`);
  }
  return segment;
}

function assertPathInside(root, candidate) {
  const resolvedRoot = path.resolve(root);
  const resolvedCandidate = path.resolve(candidate);
  const relative = path.relative(resolvedRoot, resolvedCandidate);
  if (relative && (relative.startsWith("..") || path.isAbsolute(relative))) {
    throw new Error("refusing to install outside the RepoGrammar cache");
  }
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

const MAX_DOWNLOAD_REDIRECTS = 5;

// Bound redirect following and resolve relative Location headers so a redirect
// loop (reachable via REPOGRAMMAR_RELEASE_BASE/proxy) cannot recurse until the
// stack overflows.
function resolveRedirect(location, currentUrl, redirectsRemaining) {
  if (redirectsRemaining <= 0) {
    throw new Error("download failed: too many redirects");
  }
  try {
    return new URL(location, currentUrl).toString();
  } catch (_error) {
    throw new Error("download failed: invalid redirect location");
  }
}

function download(url, destination, redirectsRemaining = MAX_DOWNLOAD_REDIRECTS) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, (response) => {
      if (
        response.statusCode >= 300 &&
        response.statusCode < 400 &&
        response.headers.location
      ) {
        response.resume();
        let nextUrl;
        try {
          nextUrl = resolveRedirect(response.headers.location, url, redirectsRemaining);
        } catch (error) {
          reject(error);
          return;
        }
        download(nextUrl, destination, redirectsRemaining - 1).then(resolve, reject);
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

function extractArchive(archivePath, destination) {
  ensureDirectory(destination);
  childProcess.execFileSync("tar", ["-xzf", archivePath, "-C", destination], {
    stdio: "ignore",
  });
}

function listArchiveEntries(archivePath) {
  const output = childProcess.execFileSync("tar", ["-tzf", archivePath], {
    encoding: "utf8",
  });
  return output.split(/\r?\n/).filter(Boolean);
}

// Reject symlink/hardlink/other non-regular members before extraction so a
// hostile archive whose checksum matches cannot redirect extraction outside the
// temp directory on older `tar` implementations. This mirrors the shell
// installer's `validate_release_archive_entries` type gate.
function assertArchiveMemberTypesAreRegular(archivePath) {
  // The first character of each `tar -tvzf` line is the member type: '-'
  // regular file and 'd' directory are the only safe types across GNU and BSD
  // tar; 'l' (symlink), 'h' (hardlink), and device/pipe/socket types are not.
  const output = childProcess.execFileSync("tar", ["-tvzf", archivePath], {
    encoding: "utf8",
  });
  for (const line of output.split(/\r?\n/)) {
    if (!line.trim()) {
      continue;
    }
    const typeChar = line[0];
    if (typeChar !== "-" && typeChar !== "d") {
      throw new Error(`release artifact contains a non-regular-file member: ${line}`);
    }
  }
}

function normalizeArchiveEntry(entry) {
  if (entry.includes("://")) {
    throw new Error(`unsafe release artifact path: ${entry}`);
  }
  let normalized = entry
    .trim()
    .replace(/\\/g, "/")
    .replace(/^\.\//, "")
    .replace(/\/+$/, "");
  if (!normalized) {
    return null;
  }
  if (
    normalized.startsWith("/") ||
    path.win32.isAbsolute(normalized) ||
    normalized.split("/").some((component) => !component || component === "." || component === "..")
  ) {
    throw new Error(`unsafe release artifact path: ${entry}`);
  }
  return normalized;
}

function validateArchiveEntries(archivePath) {
  assertArchiveMemberTypesAreRegular(archivePath);
  const allowed = new Set([
    binaryName(),
    "workers",
    "workers/python",
    "workers/python/worker.py",
  ]);
  const entries = new Set();
  for (const entry of listArchiveEntries(archivePath)) {
    const normalized = normalizeArchiveEntry(entry);
    if (!normalized) {
      continue;
    }
    if (!allowed.has(normalized)) {
      throw new Error(`unexpected release artifact path: ${entry}`);
    }
    entries.add(normalized);
  }
  if (!entries.has(binaryName())) {
    throw new Error(`release artifact did not contain ${binaryName()}`);
  }
  if (!entries.has("workers/python/worker.py")) {
    throw new Error("release artifact did not contain bundled Python worker at workers/python/worker.py");
  }
}

// After extraction, re-verify that the paths we copy into place are regular
// files and not symlinks (defense-in-depth behind the pre-extraction type gate,
// mirroring the shell installer's post-extraction `! -L` re-check).
function assertRegularFile(filePath, message) {
  const stat = fs.lstatSync(filePath, { throwIfNoEntry: false });
  if (!stat || !stat.isFile()) {
    throw new Error(message);
  }
}

function isInstalled(binary) {
  if (!fs.existsSync(binary)) {
    return false;
  }
  const worker = path.join(path.dirname(binary), "workers", "python", "worker.py");
  return fs.existsSync(worker);
}

// Activate a fully staged install without ever deleting an install directory
// that may have been won by another launcher process. A rename collision with
// another complete install is success; an incomplete/foreign collision is
// preserved and reported. Only this invocation's own backup may be restored
// or removed.
function activateStagedInstall(stagingDir, installDir, backupDir = null) {
  try {
    fs.renameSync(stagingDir, installDir);
  } catch (error) {
    if (isInstalled(path.join(installDir, binaryName()))) {
      if (backupDir) {
        fs.rmSync(backupDir, { recursive: true, force: true });
      }
      return false;
    }
    if (backupDir && !fs.existsSync(installDir) && fs.existsSync(backupDir)) {
      fs.renameSync(backupDir, installDir);
    }
    throw error;
  }
  if (backupDir) {
    fs.rmSync(backupDir, { recursive: true, force: true });
  }
  return true;
}

async function ensureBinary() {
  const target = platformTarget();
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
    validateArchiveEntries(archivePath);
    extractArchive(archivePath, tempDir);
    const extractedBinary = path.join(tempDir, binaryName());
    assertRegularFile(extractedBinary, `release artifact did not contain ${binaryName()}`);
    const workerSource = path.join(tempDir, "workers", "python", "worker.py");
    assertRegularFile(
      workerSource,
      "release artifact did not contain bundled Python worker at workers/python/worker.py"
    );
    const installParent = path.dirname(installDir);
    ensureDirectory(installParent);
    const stagingDir = fs.mkdtempSync(path.join(installParent, ".repogrammar-install-"));
    const stagedBinary = path.join(stagingDir, binaryName());
    fs.copyFileSync(extractedBinary, stagedBinary);
    fs.chmodSync(stagedBinary, 0o755);
    const workerDestination = path.join(stagingDir, "workers", "python");
    ensureDirectory(workerDestination);
    fs.copyFileSync(workerSource, path.join(workerDestination, "worker.py"));
    let backupDir = fs.existsSync(installDir)
      ? path.join(installParent, `.repogrammar-backup-${process.pid}-${Date.now()}`)
      : null;
    try {
      if (isInstalled(installed)) {
        return installed;
      }
      if (backupDir) {
        try {
          fs.renameSync(installDir, backupDir);
        } catch (error) {
          if (error.code === "ENOENT") {
            backupDir = null;
          } else {
            throw error;
          }
        }
      }
      activateStagedInstall(stagingDir, installDir, backupDir);
    } finally {
      fs.rmSync(stagingDir, { recursive: true, force: true });
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
  activateStagedInstall,
  artifactName,
  binaryPath,
  defaultReleaseTag,
  detectLinuxLibc,
  detectLinuxGlibcVersion,
  ensureBinary,
  platformTarget,
  resolveRedirect,
  validateArchiveEntries,
  validateReleaseTag,
  versionAtLeast,
  verifyChecksum,
};
