#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const childProcess = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const launcher = require("./repogrammar.js");

function mkdir(directory) {
  fs.mkdirSync(directory, { recursive: true });
}

function sha256(filePath) {
  return crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

function makeFakeRelease(root) {
  const target = launcher.platformTarget();
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(path.join(packageDir, "workers", "python"));
  const binaryPath = path.join(packageDir, process.platform === "win32" ? "repogrammar.exe" : "repogrammar");
  if (process.platform === "win32") {
    fs.writeFileSync(
      binaryPath,
      "@echo off\r\necho windows fake repogrammar is not used in default tests\r\n"
    );
  } else {
    fs.writeFileSync(
      binaryPath,
      [
        "#!/usr/bin/env sh",
        "if [ -n \"${REPOGRAMMAR_FAKE_LOG:-}\" ]; then",
        "  printf '%s' \"$1\" >> \"$REPOGRAMMAR_FAKE_LOG\"",
        "  shift",
        "  for arg in \"$@\"; do printf ' %s' \"$arg\" >> \"$REPOGRAMMAR_FAKE_LOG\"; done",
        "  printf '\\n' >> \"$REPOGRAMMAR_FAKE_LOG\"",
        "fi",
        "case \"${1:-}\" in",
        "  --version|version) echo 'repogrammar 0.1.0-test' ;;",
        "  *) exit 0 ;;",
        "esac",
        "",
      ].join("\n")
    );
    fs.chmodSync(binaryPath, 0o755);
  }
  fs.writeFileSync(
    path.join(packageDir, "workers", "python", "worker.py"),
    "print('fake worker')\n"
  );
  mkdir(releaseDir);
  const artifact = launcher.artifactName(target);
  const artifactPath = path.join(releaseDir, artifact);
  if (process.platform === "win32") {
    childProcess.execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Compress-Archive -Path ${JSON.stringify(
          path.join(packageDir, "*")
        )} -DestinationPath ${JSON.stringify(artifactPath)} -Force`,
      ],
      { stdio: "ignore" }
    );
  } else {
    childProcess.execFileSync(
      "tar",
      ["-czf", artifactPath, "-C", packageDir, "repogrammar", "workers"],
      { stdio: "ignore" }
    );
  }
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { artifact, releaseDir, target };
}

function makeFakeReleaseWithoutWorker(root) {
  const target = launcher.platformTarget();
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(packageDir);
  const binaryPath = path.join(packageDir, process.platform === "win32" ? "repogrammar.exe" : "repogrammar");
  fs.writeFileSync(
    binaryPath,
    process.platform === "win32"
      ? "@echo off\r\necho fake repogrammar\r\n"
      : "#!/usr/bin/env sh\necho fake repogrammar\n"
  );
  if (process.platform !== "win32") {
    fs.chmodSync(binaryPath, 0o755);
  }
  mkdir(releaseDir);
  const artifact = launcher.artifactName(target);
  const artifactPath = path.join(releaseDir, artifact);
  if (process.platform === "win32") {
    childProcess.execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Compress-Archive -Path ${JSON.stringify(binaryPath)} -DestinationPath ${JSON.stringify(artifactPath)} -Force`,
      ],
      { stdio: "ignore" }
    );
  } else {
    childProcess.execFileSync(
      "tar",
      ["-czf", artifactPath, "-C", packageDir, "repogrammar"],
      { stdio: "ignore" }
    );
  }
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { releaseDir };
}

function makeFakeReleaseWithUnexpectedEntry(root) {
  const target = launcher.platformTarget();
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(path.join(packageDir, "workers", "python"));
  const binaryPath = path.join(packageDir, process.platform === "win32" ? "repogrammar.exe" : "repogrammar");
  fs.writeFileSync(
    binaryPath,
    process.platform === "win32"
      ? "@echo off\r\necho fake repogrammar\r\n"
      : "#!/usr/bin/env sh\necho fake repogrammar\n"
  );
  if (process.platform !== "win32") {
    fs.chmodSync(binaryPath, 0o755);
  }
  fs.writeFileSync(
    path.join(packageDir, "workers", "python", "worker.py"),
    "print('fake worker')\n"
  );
  fs.writeFileSync(path.join(packageDir, "unexpected.txt"), "unexpected\n");
  mkdir(releaseDir);
  const artifact = launcher.artifactName(target);
  const artifactPath = path.join(releaseDir, artifact);
  if (process.platform === "win32") {
    childProcess.execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Compress-Archive -Path ${JSON.stringify(
          path.join(packageDir, "*")
        )} -DestinationPath ${JSON.stringify(artifactPath)} -Force`,
      ],
      { stdio: "ignore" }
    );
  } else {
    childProcess.execFileSync(
      "tar",
      ["-czf", artifactPath, "-C", packageDir, "repogrammar", "workers", "unexpected.txt"],
      { stdio: "ignore" }
    );
  }
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { releaseDir };
}

async function withEnv(updates, callback) {
  const previous = {};
  for (const [key, value] of Object.entries(updates)) {
    previous[key] = process.env[key];
    if (value === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = value;
    }
  }
  try {
    await callback();
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = value;
      }
    }
  }
}

async function testInstallsFromLocalReleaseAndCachesWorker() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-test-"));
  try {
    const { releaseDir, target } = makeFakeRelease(root);
    const cacheDir = path.join(root, "cache");
    await withEnv(
      {
        REPOGRAMMAR_RELEASE_DIR: releaseDir,
        REPOGRAMMAR_NPM_CACHE_DIR: cacheDir,
        REPOGRAMMAR_VERSION: "v0.1.0-test",
        REPOGRAMMAR_BINARY: undefined,
      },
      async () => {
        const binary = await launcher.ensureBinary();
        assert.equal(binary, path.join(cacheDir, "v0.1.0-test", target, process.platform === "win32" ? "repogrammar.exe" : "repogrammar"));
        assert.equal(fs.existsSync(binary), true);
        assert.equal(
          fs.existsSync(path.join(path.dirname(binary), "workers", "python", "worker.py")),
          true
        );
        assert.equal(await launcher.ensureBinary(), binary);
      }
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

async function testRejectsReleaseWithoutBundledWorker() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-worker-missing-"));
  try {
    const { releaseDir } = makeFakeReleaseWithoutWorker(root);
    const cacheDir = path.join(root, "cache");
    await withEnv(
      {
        REPOGRAMMAR_RELEASE_DIR: releaseDir,
        REPOGRAMMAR_NPM_CACHE_DIR: cacheDir,
        REPOGRAMMAR_VERSION: "v0.1.0-test",
        REPOGRAMMAR_BINARY: undefined,
      },
      async () => {
        await assert.rejects(
          () => launcher.ensureBinary(),
          /bundled Python worker/
        );
        const cachedBinary = launcher.binaryPath(launcher.platformTarget(), "v0.1.0-test");
        assert.equal(fs.existsSync(cachedBinary), false);
      }
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

async function testRejectsUnexpectedArchiveEntry() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-archive-"));
  try {
    const { releaseDir } = makeFakeReleaseWithUnexpectedEntry(root);
    const cacheDir = path.join(root, "cache");
    await withEnv(
      {
        REPOGRAMMAR_RELEASE_DIR: releaseDir,
        REPOGRAMMAR_NPM_CACHE_DIR: cacheDir,
        REPOGRAMMAR_VERSION: "v0.1.0-test",
        REPOGRAMMAR_BINARY: undefined,
      },
      async () => {
        await assert.rejects(
          () => launcher.ensureBinary(),
          /unexpected release artifact path/
        );
        const cachedBinary = launcher.binaryPath(launcher.platformTarget(), "v0.1.0-test");
        assert.equal(fs.existsSync(cachedBinary), false);
      }
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testChecksumRejectsMismatch() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-checksum-"));
  try {
    const archive = path.join(root, "archive.tar.gz");
    const checksum = path.join(root, "archive.tar.gz.sha256");
    fs.writeFileSync(archive, "payload");
    fs.writeFileSync(checksum, "0000  archive.tar.gz\n");
    assert.throws(
      () => launcher.verifyChecksum(archive, checksum),
      /checksum verification failed/
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testRejectsUnsafeReleaseTagsAndCachePaths() {
  assert.throws(() => launcher.validateReleaseTag("../outside"), /invalid RepoGrammar release tag/);
  assert.throws(() => launcher.validateReleaseTag("v0/2"), /invalid RepoGrammar release tag/);
  assert.throws(
    () => launcher.binaryPath(launcher.platformTarget(), "../outside"),
    /invalid RepoGrammar release tag/
  );
}

function testPlatformArtifactMatrixAndUnsupportedTargets() {
  const cases = [
    ["darwin", "arm64", "aarch64-apple-darwin", "repogrammar-aarch64-apple-darwin.tar.gz"],
    ["darwin", "x64", "x86_64-apple-darwin", "repogrammar-x86_64-apple-darwin.tar.gz"],
    ["linux", "arm64", "aarch64-unknown-linux-gnu", "repogrammar-aarch64-unknown-linux-gnu.tar.gz"],
    ["linux", "x64", "x86_64-unknown-linux-gnu", "repogrammar-x86_64-unknown-linux-gnu.tar.gz"],
    ["win32", "x64", "x86_64-pc-windows-msvc", "repogrammar-x86_64-pc-windows-msvc.zip"],
  ];
  for (const [platform, arch, target, artifact] of cases) {
    assert.equal(launcher.platformTarget(platform, arch), target);
    assert.equal(launcher.artifactName(target, platform), artifact);
  }

  assert.throws(
    () => launcher.platformTarget("linux", "riscv64"),
    /unsupported architecture: riscv64/
  );
  assert.throws(
    () => launcher.platformTarget("freebsd", "x64"),
    /unsupported platform: freebsd/
  );
  assert.throws(
    () => launcher.platformTarget("win32", "arm64"),
    /Windows preview supports x86_64 only/
  );
}

function testForwardsArgumentsThroughNpxLauncher() {
  if (process.platform === "win32") {
    return;
  }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-forward-"));
  try {
    const { releaseDir } = makeFakeRelease(root);
    const log = path.join(root, "fake.log");
    const result = childProcess.spawnSync(
      process.execPath,
      [
        path.join(__dirname, "repogrammar.js"),
        "install",
        "--target",
        "codex,claude-code",
        "--scope",
        "local",
        "--print-config",
      ],
      {
        env: {
          ...process.env,
          REPOGRAMMAR_RELEASE_DIR: releaseDir,
          REPOGRAMMAR_NPM_CACHE_DIR: path.join(root, "cache"),
          REPOGRAMMAR_VERSION: "v0.1.0-test",
          REPOGRAMMAR_FAKE_LOG: log,
          REPOGRAMMAR_BINARY: "",
        },
        encoding: "utf8",
      }
    );
    assert.equal(result.status, 0, result.stderr);
    assert.match(
      fs.readFileSync(log, "utf8"),
      /install --target codex,claude-code --scope local --print-config/
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testBinaryOverrideBypassesReleaseDownload() {
  if (process.platform === "win32") {
    return;
  }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-binary-"));
  try {
    const binary = path.join(root, "repogrammar");
    const log = path.join(root, "fake.log");
    fs.writeFileSync(
      binary,
      [
        "#!/usr/bin/env sh",
        "printf '%s' \"$1\" >> \"$REPOGRAMMAR_FAKE_LOG\"",
        "shift",
        "for arg in \"$@\"; do printf ' %s' \"$arg\" >> \"$REPOGRAMMAR_FAKE_LOG\"; done",
        "printf '\\n' >> \"$REPOGRAMMAR_FAKE_LOG\"",
        "",
      ].join("\n")
    );
    fs.chmodSync(binary, 0o755);

    const result = childProcess.spawnSync(
      process.execPath,
      [path.join(__dirname, "repogrammar.js"), "install", "--dry-run"],
      {
        env: {
          ...process.env,
          REPOGRAMMAR_BINARY: binary,
          REPOGRAMMAR_RELEASE_DIR: path.join(root, "missing-release-dir"),
          REPOGRAMMAR_FAKE_LOG: log,
        },
        encoding: "utf8",
      }
    );

    assert.equal(result.status, 0, result.stderr);
    assert.equal(fs.readFileSync(log, "utf8"), "install --dry-run\n");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

async function main() {
  testPlatformArtifactMatrixAndUnsupportedTargets();
  testChecksumRejectsMismatch();
  testRejectsUnsafeReleaseTagsAndCachePaths();
  await testInstallsFromLocalReleaseAndCachesWorker();
  await testRejectsReleaseWithoutBundledWorker();
  await testRejectsUnexpectedArchiveEntry();
  testForwardsArgumentsThroughNpxLauncher();
  testBinaryOverrideBypassesReleaseDownload();
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
