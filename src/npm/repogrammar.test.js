#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const childProcess = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const launcher = require("./repogrammar.js");
const packageJson = require("../../package.json");

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
  const binaryPath = path.join(packageDir, "repogrammar");
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
  fs.writeFileSync(
    path.join(packageDir, "workers", "python", "worker.py"),
    "print('fake worker')\n"
  );
  mkdir(releaseDir);
  const artifact = launcher.artifactName(target);
  const artifactPath = path.join(releaseDir, artifact);
  childProcess.execFileSync(
    "tar",
    ["-czf", artifactPath, "-C", packageDir, "repogrammar", "workers"],
    { stdio: "ignore" }
  );
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { artifact, releaseDir, target };
}

function makeFakeReleaseWithoutWorker(root) {
  const target = launcher.platformTarget();
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(packageDir);
  const binaryPath = path.join(packageDir, "repogrammar");
  fs.writeFileSync(binaryPath, "#!/usr/bin/env sh\necho fake repogrammar\n");
  fs.chmodSync(binaryPath, 0o755);
  mkdir(releaseDir);
  const artifact = launcher.artifactName(target);
  const artifactPath = path.join(releaseDir, artifact);
  childProcess.execFileSync(
    "tar",
    ["-czf", artifactPath, "-C", packageDir, "repogrammar"],
    { stdio: "ignore" }
  );
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { releaseDir };
}

function makeFakeReleaseWithUnexpectedEntry(root) {
  const target = launcher.platformTarget();
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(path.join(packageDir, "workers", "python"));
  const binaryPath = path.join(packageDir, "repogrammar");
  fs.writeFileSync(binaryPath, "#!/usr/bin/env sh\necho fake repogrammar\n");
  fs.chmodSync(binaryPath, 0o755);
  fs.writeFileSync(
    path.join(packageDir, "workers", "python", "worker.py"),
    "print('fake worker')\n"
  );
  fs.writeFileSync(path.join(packageDir, "unexpected.txt"), "unexpected\n");
  mkdir(releaseDir);
  const artifact = launcher.artifactName(target);
  const artifactPath = path.join(releaseDir, artifact);
  childProcess.execFileSync(
    "tar",
    ["-czf", artifactPath, "-C", packageDir, "repogrammar", "workers", "unexpected.txt"],
    { stdio: "ignore" }
  );
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { releaseDir };
}

// Build a tar release whose `repogrammar` member is a SYMLINK (allowed name,
// non-regular type). Unix tar records it with a leading 'l' in `-tvzf`. Used to
// prove the launcher rejects a hostile archive before extraction even when the
// member name is whitelisted. Non-Windows only (relies on tar + symlinks).
function makeFakeReleaseWithSymlinkMember(root) {
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(path.join(packageDir, "workers", "python"));
  fs.symlinkSync("outside-target", path.join(packageDir, "repogrammar"));
  fs.writeFileSync(
    path.join(packageDir, "workers", "python", "worker.py"),
    "print('fake worker')\n"
  );
  mkdir(releaseDir);
  const artifact = launcher.artifactName(launcher.platformTarget());
  const artifactPath = path.join(releaseDir, artifact);
  childProcess.execFileSync(
    "tar",
    ["-czf", artifactPath, "-C", packageDir, "repogrammar", "workers"],
    { stdio: "ignore" }
  );
  fs.writeFileSync(path.join(releaseDir, `${artifact}.sha256`), `${sha256(artifactPath)}  ${artifact}\n`);
  return { releaseDir };
}

// Build a tar release whose `workers/python/worker.py` member is a HARDLINK to
// the regular `repogrammar` binary. Unix tar records the second reference to the
// same inode with a leading 'h' in `-tvzf`. Both member names are whitelisted,
// so acceptance would depend solely on the type gate. Non-Windows only.
function makeFakeReleaseWithHardlinkMember(root) {
  const releaseDir = path.join(root, "release");
  const packageDir = path.join(root, "package");
  mkdir(path.join(packageDir, "workers", "python"));
  const binaryPath = path.join(packageDir, "repogrammar");
  fs.writeFileSync(binaryPath, "#!/usr/bin/env sh\necho fake repogrammar\n");
  fs.chmodSync(binaryPath, 0o755);
  fs.linkSync(binaryPath, path.join(packageDir, "workers", "python", "worker.py"));
  mkdir(releaseDir);
  const artifact = launcher.artifactName(launcher.platformTarget());
  const artifactPath = path.join(releaseDir, artifact);
  childProcess.execFileSync(
    "tar",
    ["-czf", artifactPath, "-C", packageDir, "repogrammar", "workers"],
    { stdio: "ignore" }
  );
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
        assert.equal(binary, path.join(cacheDir, "v0.1.0-test", target, "repogrammar"));
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

async function testRejectsReleaseWithSymlinkMember() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-symlink-"));
  try {
    const { releaseDir } = makeFakeReleaseWithSymlinkMember(root);
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
          /non-regular-file member/
        );
        // The hostile archive must be rejected before extraction leaves any
        // cached command binary behind.
        const cachedBinary = launcher.binaryPath(launcher.platformTarget(), "v0.1.0-test");
        assert.equal(fs.existsSync(cachedBinary), false);
        assert.equal(fs.existsSync(path.dirname(cachedBinary)), false);
      }
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

async function testRejectsReleaseWithHardlinkMember() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-hardlink-"));
  try {
    const { releaseDir } = makeFakeReleaseWithHardlinkMember(root);
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
          /non-regular-file member/
        );
        const cachedBinary = launcher.binaryPath(launcher.platformTarget(), "v0.1.0-test");
        assert.equal(fs.existsSync(cachedBinary), false);
        assert.equal(fs.existsSync(path.dirname(cachedBinary)), false);
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
  assert.equal(packageJson.version, "0.3.0");
  assert.deepEqual(packageJson.os, ["darwin", "linux"]);
  assert.deepEqual(packageJson.cpu, ["x64", "arm64"]);
  // npm applies a root `libc` restriction to Darwin too (where libc is
  // undefined), so one Darwin/Linux package cannot express Linux-only glibc
  // support through standard manifest fields. The launcher owns that gate.
  assert.equal(Object.hasOwn(packageJson, "libc"), false);
  assert.equal(packageJson.repository.url, "git+https://github.com/SioYooo/RepoGrammar.git");
  assert.equal(packageJson.homepage, "https://github.com/SioYooo/RepoGrammar#readme");
  assert.equal(packageJson.bugs.url, "https://github.com/SioYooo/RepoGrammar/issues");
  const cases = [
    ["darwin", "arm64", null, "aarch64-apple-darwin", "repogrammar-aarch64-apple-darwin.tar.gz"],
    ["darwin", "x64", null, "x86_64-apple-darwin", "repogrammar-x86_64-apple-darwin.tar.gz"],
    ["linux", "arm64", "glibc", "2.39", "aarch64-unknown-linux-gnu", "repogrammar-aarch64-unknown-linux-gnu.tar.gz"],
    ["linux", "x64", "glibc", "2.35", "x86_64-unknown-linux-gnu", "repogrammar-x86_64-unknown-linux-gnu.tar.gz"],
  ];
  for (const entry of cases) {
    const [platform, arch, libc, glibcVersion, target, artifact] =
      entry.length === 6 ? entry : [entry[0], entry[1], entry[2], null, entry[3], entry[4]];
    assert.equal(launcher.platformTarget(platform, arch, libc, glibcVersion), target);
    assert.equal(launcher.artifactName(target), artifact);
  }

  assert.throws(
    () => launcher.platformTarget("linux", "x64", "musl", null),
    /unsupported Linux runtime: musl.*requires glibc/
  );
  assert.throws(
    () => launcher.platformTarget("linux", "x64", "unknown", null),
    /unsupported Linux runtime: unknown libc.*requires glibc/
  );

  assert.throws(
    () => launcher.platformTarget("linux", "riscv64"),
    /unsupported architecture: riscv64/
  );
  assert.throws(
    () => launcher.platformTarget("freebsd", "x64"),
    /unsupported platform: freebsd/
  );
  assert.throws(
    () => launcher.platformTarget("win32", "x64"),
    /unsupported platform: win32.*macOS and Linux only/
  );
  assert.throws(
    () => launcher.platformTarget("win32", "arm64"),
    /unsupported platform: win32.*macOS and Linux only/
  );
  assert.throws(
    () => launcher.platformTarget("linux", "x64", "glibc", "2.34"),
    /x64 release binaries require glibc 2\.35\+/
  );
  assert.throws(
    () => launcher.platformTarget("linux", "arm64", "glibc", "2.38"),
    /arm64 release binaries require glibc 2\.39\+/
  );
}

function testLinuxLibcReportClassification() {
  assert.equal(
    launcher.detectLinuxLibc({ header: { glibcVersionRuntime: "2.39" }, sharedObjects: [] }),
    "glibc"
  );
  assert.equal(
    launcher.detectLinuxLibc({
      header: {},
      sharedObjects: ["/lib/ld-musl-x86_64.so.1"],
    }),
    "musl"
  );
  assert.equal(launcher.detectLinuxLibc({ header: {}, sharedObjects: [] }), "unknown");
  assert.equal(
    launcher.detectLinuxGlibcVersion({ header: { glibcVersionRuntime: "2.39" } }),
    "2.39"
  );
  assert.equal(launcher.detectLinuxGlibcVersion({ header: {} }), null);
  assert.equal(launcher.versionAtLeast("2.35", "2.35"), true);
  assert.equal(launcher.versionAtLeast("2.39", "2.35"), true);
  assert.equal(launcher.versionAtLeast("2.34", "2.35"), false);
  assert.equal(launcher.versionAtLeast("unknown", "2.35"), false);
}

function testConcurrentActivationPreservesWinningInstall() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-race-"));
  try {
    const staging = path.join(root, "staging");
    const installDir = path.join(root, "install");
    mkdir(path.join(staging, "workers", "python"));
    fs.writeFileSync(path.join(staging, "repogrammar"), "losing staged binary\n");
    fs.writeFileSync(path.join(staging, "workers", "python", "worker.py"), "losing worker\n");

    // Simulate a concurrent launcher winning the first-install rename after
    // this launcher staged its files but before it attempts activation.
    mkdir(path.join(installDir, "workers", "python"));
    fs.writeFileSync(path.join(installDir, "repogrammar"), "winning binary\n");
    fs.writeFileSync(path.join(installDir, "workers", "python", "worker.py"), "winning worker\n");

    const activated = launcher.activateStagedInstall(staging, installDir);
    assert.equal(activated, false);
    assert.equal(fs.readFileSync(path.join(installDir, "repogrammar"), "utf8"), "winning binary\n");
    assert.equal(
      fs.readFileSync(path.join(installDir, "workers", "python", "worker.py"), "utf8"),
      "winning worker\n"
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testPackedNpmTarballInstallsAndExecutesOffline() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "repogrammar-npm-pack-"));
  try {
    const repoRoot = path.resolve(__dirname, "../..");
    const pack = childProcess.spawnSync(
      "npm",
      ["pack", "--json", "--ignore-scripts", "--pack-destination", root],
      {
        cwd: repoRoot,
        env: { ...process.env, npm_config_cache: path.join(root, "npm-cache") },
        encoding: "utf8",
      }
    );
    assert.equal(pack.status, 0, pack.stderr);
    const packResult = JSON.parse(pack.stdout);
    assert.equal(packResult.length, 1);
    const tarball = path.join(root, packResult[0].filename);
    assert.equal(fs.existsSync(tarball), true);

    const entries = childProcess
      .execFileSync("tar", ["-tzf", tarball], { encoding: "utf8" })
      .trim()
      .split(/\r?\n/)
      .sort();
    assert.deepEqual(entries, [
      "package/LICENSE",
      "package/README.md",
      "package/package.json",
      "package/src/npm/repogrammar.js",
    ]);

    const unpacked = path.join(root, "unpacked");
    mkdir(unpacked);
    childProcess.execFileSync("tar", ["-xzf", tarball, "-C", unpacked]);
    const packedManifest = JSON.parse(
      fs.readFileSync(path.join(unpacked, "package", "package.json"), "utf8")
    );
    assert.equal(Object.hasOwn(packedManifest, "libc"), false);
    assert.equal(packedManifest.repository.url, packageJson.repository.url);
    const packedReadme = fs.readFileSync(path.join(unpacked, "package", "README.md"), "utf8");
    assert.doesNotMatch(packedReadme, /\]\((?:docs\/|CONTRIBUTING\.md|SECURITY\.md|CODE_OF_CONDUCT\.md|LICENSE\))/);

    const installPrefix = path.join(root, "installed");
    const install = childProcess.spawnSync(
      "npm",
      [
        "install",
        "--global",
        "--prefix",
        installPrefix,
        "--ignore-scripts",
        "--offline",
        "--no-audit",
        "--no-fund",
        tarball,
      ],
      {
        env: { ...process.env, npm_config_cache: path.join(root, "npm-cache") },
        encoding: "utf8",
      }
    );
    assert.equal(install.status, 0, install.stderr);

    const { releaseDir } = makeFakeRelease(path.join(root, "fake-release"));
    const executable = path.join(installPrefix, "bin", "repogrammar");
    const run = childProcess.spawnSync(executable, ["version"], {
      env: {
        ...process.env,
        REPOGRAMMAR_RELEASE_DIR: releaseDir,
        REPOGRAMMAR_NPM_CACHE_DIR: path.join(root, "launcher-cache"),
        REPOGRAMMAR_VERSION: "v0.1.0-test",
        REPOGRAMMAR_BINARY: "",
      },
      encoding: "utf8",
    });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stdout.trim(), "repogrammar 0.1.0-test");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testForwardsInstallerAndSetupArgumentsThroughNpxLauncher() {
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

    fs.writeFileSync(log, "");
    const setupResult = childProcess.spawnSync(
      process.execPath,
      [
        path.join(__dirname, "repogrammar.js"),
        "setup",
        "--project",
        ".",
        "--target",
        "auto",
        "--yes",
        "--dry-run",
        "--no-autosync",
        "--json",
        "--progress",
        "never",
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
    assert.equal(setupResult.status, 0, setupResult.stderr);
    assert.match(
      fs.readFileSync(log, "utf8"),
      /setup --project \. --target auto --yes --dry-run --no-autosync --json --progress never/
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testBinaryOverrideBypassesReleaseDownload() {
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

function testRedirectResolutionIsBoundedAndRelativeAware() {
  // Absolute Location is followed as-is while budget remains.
  assert.equal(
    launcher.resolveRedirect(
      "https://cdn.example.com/final.tar.gz",
      "https://github.com/owner/repo/releases/download/v1/a.tar.gz",
      5
    ),
    "https://cdn.example.com/final.tar.gz"
  );
  // Relative Location resolves against the current URL, not the stack.
  assert.equal(
    launcher.resolveRedirect(
      "/redirected/final.tar.gz",
      "https://github.com/owner/repo/a.tar.gz",
      3
    ),
    "https://github.com/redirected/final.tar.gz"
  );
  // Exhausted redirect budget fails closed instead of recursing forever.
  assert.throws(
    () => launcher.resolveRedirect("https://example.com/loop", "https://example.com/loop", 0),
    /too many redirects/
  );
  // A malformed Location with no base to resolve against is rejected.
  assert.throws(
    () => launcher.resolveRedirect("::not a url::", "not-a-base", 5),
    /invalid redirect location/
  );
}

async function main() {
  testPlatformArtifactMatrixAndUnsupportedTargets();
  testLinuxLibcReportClassification();
  testConcurrentActivationPreservesWinningInstall();
  if (process.platform === "win32") {
    return;
  }
  testChecksumRejectsMismatch();
  testRejectsUnsafeReleaseTagsAndCachePaths();
  testRedirectResolutionIsBoundedAndRelativeAware();
  await testInstallsFromLocalReleaseAndCachesWorker();
  await testRejectsReleaseWithoutBundledWorker();
  await testRejectsUnexpectedArchiveEntry();
  await testRejectsReleaseWithSymlinkMember();
  await testRejectsReleaseWithHardlinkMember();
  testForwardsInstallerAndSetupArgumentsThroughNpxLauncher();
  testBinaryOverrideBypassesReleaseDownload();
  testPackedNpmTarballInstallsAndExecutesOffline();
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
