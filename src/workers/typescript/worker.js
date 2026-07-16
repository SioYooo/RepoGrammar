#!/usr/bin/env node
"use strict";

const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const PROTOCOL_VERSION = 1;
const DEFAULT_REQUEST_ID = "repogrammar-typescript-semantic-worker";
const ZERO_HASH =
  "sha256:0000000000000000000000000000000000000000000000000000000000000000";
const MAX_STDIN_BYTES = 1_048_576;
const MAX_PROJECT_ROOT_CHARS = 4096;
const MAX_CHANGED_FILES = 10_000;
const MAX_CHANGED_FILE_CHARS = 4096;
const MAX_OPERATIONS = 20_000;
const HASH_PATTERN = /^sha256:[A-Fa-f0-9]{64}$/;
const OPERATION_TOKENS = new Set([
  "resolve_module_specifier",
  "resolve_export",
  "resolve_reexport",
  "resolve_package_entry",
]);

function readStdin() {
  const chunks = [];
  const buffer = Buffer.allocUnsafe(64 * 1024);
  let totalBytes = 0;

  for (;;) {
    const bytesRead = fs.readSync(0, buffer, 0, buffer.length, null);
    if (bytesRead === 0) {
      break;
    }
    totalBytes += bytesRead;
    if (totalBytes > MAX_STDIN_BYTES) {
      throw new Error("semantic worker request is too large");
    }
    chunks.push(Buffer.from(buffer.subarray(0, bytesRead)));
  }

  return Buffer.concat(chunks, totalBytes).toString("utf8");
}

function isNonBlankString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function hasControlOrUriText(value) {
  return /[\u0000-\u001F]/.test(value) || value.includes("://");
}

function looksLikeWindowsAbsolutePath(value) {
  return /^[A-Za-z]:[\\/]/.test(value);
}

function hasWindowsDrivePrefix(value) {
  return /^[A-Za-z]:/.test(value);
}

function isAbsoluteProjectRoot(value) {
  return (
    isNonBlankString(value) &&
    value.length <= MAX_PROJECT_ROOT_CHARS &&
    !hasControlOrUriText(value) &&
    (value.startsWith("/") || looksLikeWindowsAbsolutePath(value))
  );
}

function isSafeRepoRelativePath(value) {
  if (
    !isNonBlankString(value) ||
    value.length > MAX_CHANGED_FILE_CHARS ||
    hasControlOrUriText(value) ||
    value.startsWith("/") ||
    value.includes("\\") ||
    hasWindowsDrivePrefix(value)
  ) {
    return false;
  }

  return value.split("/").every((segment) => {
    return segment.length > 0 && segment !== "." && segment !== "..";
  });
}

function isSafeProtocolText(value) {
  if (!isNonBlankString(value) || hasControlOrUriText(value)) {
    return false;
  }
  if (value.split(/\s+/).some((part) => part.startsWith("/") || hasWindowsDrivePrefix(part))) {
    return false;
  }
  const trimmed = value.trimStart();
  return !(
    value.includes("=>") ||
    (value.includes("=") && value.includes(";")) ||
    value.includes("{") ||
    value.includes("}") ||
    trimmed.startsWith("const ") ||
    trimmed.startsWith("let ") ||
    trimmed.startsWith("var ") ||
    trimmed.startsWith("import ") ||
    trimmed.startsWith("export ")
  );
}

function requestIdFor(payload) {
  if (payload && payload.request_id === DEFAULT_REQUEST_ID) {
    return payload.request_id;
  }
  return DEFAULT_REQUEST_ID;
}

function validateRequest(payload) {
  if (!payload || Array.isArray(payload) || typeof payload !== "object") {
    return false;
  }

  const allowedFields = new Set([
    "protocol_version",
    "request_id",
    "project_root",
    "changed_files",
    "operations",
  ]);
  for (const field of Object.keys(payload)) {
    if (!allowedFields.has(field)) {
      return false;
    }
  }

  if (payload.protocol_version !== PROTOCOL_VERSION) {
    return false;
  }
  if (payload.request_id !== DEFAULT_REQUEST_ID) {
    return false;
  }
  if (!isAbsoluteProjectRoot(payload.project_root)) {
    return false;
  }
  if (
    !Array.isArray(payload.changed_files) ||
    payload.changed_files.length > MAX_CHANGED_FILES
  ) {
    return false;
  }
  const seen = new Set();
  for (const changedFile of payload.changed_files) {
    if (!isSafeRepoRelativePath(changedFile) || seen.has(changedFile)) {
      return false;
    }
    seen.add(changedFile);
  }

  if (!Array.isArray(payload.operations) || payload.operations.length > MAX_OPERATIONS) {
    return false;
  }
  return payload.operations.every(validateOperation);
}

function validateOperation(operation) {
  if (!operation || Array.isArray(operation) || typeof operation !== "object") {
    return false;
  }
  const allowedFields = new Set([
    "operation_id",
    "operation",
    "path",
    "content_hash",
    "code_unit_id",
    "start_byte",
    "end_byte",
    "literal_specifier",
    "project_config_hash",
    "package_json_hash",
    "max_files",
    "max_bytes",
  ]);
  for (const field of Object.keys(operation)) {
    if (!allowedFields.has(field)) {
      return false;
    }
  }
  return (
    OPERATION_TOKENS.has(operation.operation) &&
    isSafeProtocolText(operation.operation_id) &&
    isSafeRepoRelativePath(operation.path) &&
    HASH_PATTERN.test(operation.content_hash) &&
    isSafeProtocolText(operation.code_unit_id) &&
    Number.isSafeInteger(operation.start_byte) &&
    Number.isSafeInteger(operation.end_byte) &&
    operation.start_byte <= operation.end_byte &&
    isSafeProtocolText(operation.literal_specifier) &&
    HASH_PATTERN.test(operation.project_config_hash) &&
    HASH_PATTERN.test(operation.package_json_hash) &&
    Number.isSafeInteger(operation.max_files) &&
    operation.max_files > 0 &&
    Number.isSafeInteger(operation.max_bytes) &&
    operation.max_bytes > 0
  );
}

function message(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

function emitWorkerError(requestId, errorCode, text) {
  message({
    protocol_version: PROTOCOL_VERSION,
    message_type: "worker_error",
    request_id: requestId,
    error_code: errorCode,
    message: text,
    fallback: {
      mode: "syntax_only",
      certainty: "UNKNOWN",
    },
  });
  emitEnd(requestId);
}

function emitEnd(requestId) {
  message({
    protocol_version: PROTOCOL_VERSION,
    message_type: "end_of_stream",
    request_id: requestId,
  });
}

function safePath(root, relativePath) {
  if (!isSafeRepoRelativePath(relativePath)) {
    return null;
  }
  const resolved = path.resolve(root, relativePath);
  const normalizedRoot = path.resolve(root);
  if (resolved !== normalizedRoot && !resolved.startsWith(`${normalizedRoot}${path.sep}`)) {
    return null;
  }
  return resolved;
}

function readBoundedText(root, relativePath, maxBytes) {
  const absolute = safePath(root, relativePath);
  if (!absolute) {
    return null;
  }
  let stat;
  try {
    stat = fs.statSync(absolute);
  } catch (_error) {
    return null;
  }
  if (!stat.isFile() || stat.size > maxBytes) {
    return null;
  }
  return fs.readFileSync(absolute, "utf8");
}

function hashText(text) {
  return `sha256:${crypto.createHash("sha256").update(text, "utf8").digest("hex")}`;
}

function parseJsonText(text) {
  try {
    const value = JSON.parse(text);
    return value && typeof value === "object" && !Array.isArray(value) ? value : null;
  } catch (_error) {
    return null;
  }
}

function loadProjectModel(payload) {
  const files = new Set(payload.changed_files);
  const configPath = files.has("tsconfig.json")
    ? "tsconfig.json"
    : files.has("jsconfig.json")
      ? "jsconfig.json"
      : null;
  const configText = configPath ? readBoundedText(payload.project_root, configPath, 1_048_576) : null;
  const config = configText ? parseJsonText(configText) : null;
  const packageText = files.has("package.json")
    ? readBoundedText(payload.project_root, "package.json", 1_048_576)
    : null;
  const packageJson = packageText ? parseJsonText(packageText) : null;

  return {
    projectRoot: payload.project_root,
    modulePaths: [...files].filter(isTsJsModulePath).sort(),
    configPath,
    config,
    configMalformed: Boolean(configText && !config),
    configHash: configText ? hashText(configText) : ZERO_HASH,
    packageJson,
    packageHash: packageText ? hashText(packageText) : ZERO_HASH,
    packageDependencies: packageDependencies(packageJson),
  };
}

// Loading the analyzed repository's own `typescript` package runs that
// package's top-level module code in this process — arbitrary code execution
// when indexing an untrusted repo. So the project's TypeScript is loaded ONLY
// when the operator explicitly opts in by setting
// REPOGRAMMAR_TSJS_TRUST_PROJECT_TYPESCRIPT=1 (i.e. the analyzed repo is
// trusted). By default the worker uses only its own bundled TypeScript, and
// when none is available compiler-API mode is disabled and callers fall back to
// structural resolution (every consumer guards on `model.typescript`).
function projectTypeScriptIsTrusted() {
  return process.env.REPOGRAMMAR_TSJS_TRUST_PROJECT_TYPESCRIPT === "1";
}

function loadTypeScript(projectRoot) {
  const candidates = [() => require("typescript")];
  if (projectTypeScriptIsTrusted()) {
    candidates.push(() =>
      require(require.resolve("typescript", { paths: [projectRoot] }))
    );
  }
  for (const candidate of candidates) {
    try {
      const api = candidate();
      if (typescriptApiIsUsable(api)) {
        return api;
      }
    } catch (_error) {
      // Try the next configured compiler location.
    }
  }
  return null;
}

function typescriptApiIsUsable(api) {
  return (
    api &&
    typeof api.version === "string" &&
    typeof api.resolveModuleName === "function"
  );
}

function compilerOptionsForTypeScript(model, ts) {
  if (!model.config || typeof ts.parseJsonConfigFileContent !== "function") {
    return compilerOptions(model);
  }
  try {
    const host = {
      // Bound config parsing (including tsconfig `extends`) to the project root
      // so a hostile `"extends": "/etc/..."` cannot read files outside the repo.
      fileExists: (filePath) => {
        const relative = repoRelativeFromAbsolute(model.projectRoot, filePath);
        if (!relative) {
          return false;
        }
        const absolute = safePath(model.projectRoot, relative);
        return absolute ? fs.existsSync(absolute) : false;
      },
      readFile: (filePath) => {
        const relative = repoRelativeFromAbsolute(model.projectRoot, filePath);
        if (!relative) {
          return undefined;
        }
        return readBoundedText(model.projectRoot, relative, 1_048_576) || undefined;
      },
      readDirectory: () => [],
      useCaseSensitiveFileNames: true,
    };
    const basePath = model.configPath
      ? path.dirname(path.join(model.projectRoot, model.configPath))
      : model.projectRoot;
    const parsed = ts.parseJsonConfigFileContent(model.config, host, basePath);
    return parsed && parsed.options && typeof parsed.options === "object"
      ? parsed.options
      : compilerOptions(model);
  } catch (_error) {
    return compilerOptions(model);
  }
}

function compilerHostForModel(model) {
  return {
    fileExists: (filePath) => {
      const relative = repoRelativeFromAbsolute(model.projectRoot, filePath);
      return relative ? model.modulePaths.includes(relative) : fs.existsSync(filePath);
    },
    readFile: (filePath) => {
      const relative = repoRelativeFromAbsolute(model.projectRoot, filePath);
      if (!relative) {
        return undefined;
      }
      return readBoundedText(model.projectRoot, relative, 1_048_576) || undefined;
    },
    directoryExists: (directoryPath) => {
      const relative = repoRelativeFromAbsolute(model.projectRoot, directoryPath);
      if (relative === "") {
        return true;
      }
      return relative
        ? model.modulePaths.some((modulePath) => modulePath.startsWith(`${relative}/`))
        : false;
    },
    realpath: (filePath) => filePath,
    getCurrentDirectory: () => model.projectRoot,
    getDirectories: () => [],
  };
}

function repoRelativeFromAbsolute(root, absolutePath) {
  const normalizedRoot = path.resolve(root);
  const resolved = path.resolve(absolutePath);
  if (resolved === normalizedRoot) {
    return "";
  }
  if (!resolved.startsWith(`${normalizedRoot}${path.sep}`)) {
    return null;
  }
  const relative = path.relative(normalizedRoot, resolved).replace(/\\/g, "/");
  return relative === "" || isSafeRepoRelativePath(relative) ? relative : null;
}

function resolveWithTypeScript(model, ts, currentPath, specifier) {
  const containingFile = path.join(model.projectRoot, currentPath);
  let resolvedModule;
  try {
    const result = ts.resolveModuleName(
      specifier,
      containingFile,
      compilerOptionsForTypeScript(model, ts),
      compilerHostForModel(model)
    );
    resolvedModule = result && result.resolvedModule;
  } catch (_error) {
    return null;
  }
  if (!resolvedModule || !resolvedModule.resolvedFileName) {
    return null;
  }
  const relative = repoRelativeFromAbsolute(model.projectRoot, resolvedModule.resolvedFileName);
  if (!relative || !model.modulePaths.includes(relative)) {
    return null;
  }
  return {
    kind: "resolved",
    path: relative,
    resolutionKind: "compiler_api",
    provider: "compiler",
  };
}

function packageDependencies(packageJson) {
  const dependencies = new Set();
  if (!packageJson) {
    return dependencies;
  }
  for (const field of ["dependencies", "devDependencies", "peerDependencies"]) {
    const object = packageJson[field];
    if (!object || typeof object !== "object" || Array.isArray(object)) {
      continue;
    }
    for (const name of Object.keys(object)) {
      dependencies.add(name);
    }
  }
  return dependencies;
}

function compilerOptions(model) {
  const options = model.config && model.config.compilerOptions;
  return options && typeof options === "object" && !Array.isArray(options) ? options : {};
}

function baseUrl(model) {
  const value = compilerOptions(model).baseUrl;
  if (!isNonBlankString(value)) {
    return "";
  }
  const normalized = normalizeConfigPath(value);
  return normalized && isSafeRepoRelativePath(normalized) ? normalized : "";
}

function rootDirs(model) {
  const value = compilerOptions(model).rootDirs;
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter(isNonBlankString)
    .map(normalizeConfigPath)
    .filter((entry) => entry && !entry.includes("*") && !entry.includes("?"))
    .filter(isSafeRepoRelativePath)
    .sort();
}

function pathAliases(model) {
  const paths = compilerOptions(model).paths;
  if (!paths || typeof paths !== "object" || Array.isArray(paths)) {
    return [];
  }
  const prefix = baseUrl(model);
  const aliases = [];
  for (const [aliasPattern, rawTargets] of Object.entries(paths)) {
    if (!Array.isArray(rawTargets) || !isNonBlankString(aliasPattern)) {
      continue;
    }
    if (wildcardCount(aliasPattern) > 1) {
      continue;
    }
    const targetPatterns = rawTargets
      .filter(isNonBlankString)
      .map(normalizeConfigPath)
      .map((target) => (prefix ? `${prefix}/${target}` : target))
      .filter((target) => wildcardCount(target) <= 1)
      .filter(isSafeRepoRelativePath);
    aliases.push({ aliasPattern, targetPatterns });
  }
  aliases.sort((left, right) => left.aliasPattern.localeCompare(right.aliasPattern));
  return aliases;
}

function wildcardCount(value) {
  return value.split("*").length - 1;
}

function normalizeConfigPath(value) {
  return value.trim().replace(/^\.\//, "").replace(/\/+$/, "");
}

function isTsJsModulePath(value) {
  return /\.(?:ts|tsx|js|jsx)$/.test(value);
}

function parentDir(filePath) {
  const slash = filePath.lastIndexOf("/");
  return slash === -1 ? "" : filePath.slice(0, slash);
}

function normalizeRelativeSpecifier(currentPath, specifier) {
  const parts = [...parentDir(currentPath).split("/").filter(Boolean)];
  for (const part of specifier.split("/")) {
    if (!part || part === ".") {
      continue;
    }
    if (part === "..") {
      if (parts.length === 0) {
        return null;
      }
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  const normalized = parts.join("/");
  return normalized && isSafeRepoRelativePath(normalized) ? normalized : null;
}

function modulePathCandidates(base) {
  if (/\.(?:ts|tsx|js|jsx)$/.test(base)) {
    return [base];
  }
  const candidates = [];
  for (const extension of [".ts", ".tsx", ".js", ".jsx"]) {
    candidates.push(`${base}${extension}`);
  }
  for (const extension of [".ts", ".tsx", ".js", ".jsx"]) {
    candidates.push(`${base}/index${extension}`);
  }
  return candidates;
}

function resolveModuleBase(base, modulePaths, resolutionKind) {
  const matches = modulePathCandidates(base).filter((candidate) => modulePaths.has(candidate));
  const unique = [...new Set(matches)].sort();
  if (unique.length === 1) {
    return { kind: "resolved", path: unique[0], resolutionKind };
  }
  if (unique.length > 1) {
    return { kind: "unknown", reason: "ConflictingFacts", unknownKind: "ambiguous_import" };
  }
  return { kind: "unknown", reason: "UnresolvedImport", unknownKind: "unresolved_import" };
}

function aliasReplacements(specifier, aliasPattern) {
  if (!aliasPattern.includes("*")) {
    return specifier === aliasPattern ? [""] : null;
  }
  if (wildcardCount(aliasPattern) !== 1) {
    return null;
  }
  const [prefix, suffix] = aliasPattern.split("*");
  if (!specifier.startsWith(prefix) || !specifier.endsWith(suffix)) {
    return null;
  }
  return [specifier.slice(prefix.length, specifier.length - suffix.length)];
}

function applyAliasTarget(targetPattern, replacement) {
  return targetPattern.includes("*") ? targetPattern.split("*").join(replacement) : targetPattern;
}

function resolveRootDirs(currentPath, base, dirs, modulePaths) {
  const currentRoot = dirs
    .filter((dir) => currentPath.startsWith(`${dir}/`))
    .sort((left, right) => right.length - left.length)[0];
  if (!currentRoot || !base.startsWith(`${currentRoot}/`)) {
    return null;
  }
  const suffix = base.slice(currentRoot.length + 1);
  const matches = [];
  let sawConflict = false;
  for (const dir of dirs) {
    const resolution = resolveModuleBase(`${dir}/${suffix}`, modulePaths, "root_dirs");
    if (resolution.kind === "resolved") {
      matches.push(resolution.path);
    } else if (resolution.reason === "ConflictingFacts") {
      sawConflict = true;
    }
  }
  const unique = [...new Set(matches)].sort();
  if (sawConflict || unique.length > 1) {
    return { kind: "unknown", reason: "ConflictingFacts", unknownKind: "root_dirs_conflict" };
  }
  if (unique.length === 1) {
    return { kind: "resolved", path: unique[0], resolutionKind: "root_dirs" };
  }
  return { kind: "unknown", reason: "UnresolvedImport", unknownKind: "unresolved_root_dirs" };
}

function resolveSpecifier(model, currentPath, specifier) {
  if (model.typescript) {
    const compilerResolution = resolveWithTypeScript(
      model,
      model.typescript,
      currentPath,
      specifier
    );
    if (compilerResolution) {
      return compilerResolution;
    }
  }

  const modulePaths = new Set(model.modulePaths);
  if (specifier.startsWith("./") || specifier.startsWith("../")) {
    const base = normalizeRelativeSpecifier(currentPath, specifier);
    if (!base) {
      return { kind: "unknown", reason: "UnresolvedImport", unknownKind: "unresolved_import" };
    }
    const direct = resolveModuleBase(base, modulePaths, "literal_relative");
    if (direct.kind === "resolved" || direct.reason === "ConflictingFacts") {
      return direct;
    }
    return resolveRootDirs(currentPath, base, rootDirs(model), modulePaths) || direct;
  }

  const prefix = baseUrl(model);
  if (model.configMalformed && !specifier.startsWith("#")) {
    return {
      kind: "unknown",
      reason: "MissingProjectConfig",
      unknownKind: "malformed_project_config",
    };
  }
  if (prefix) {
    const baseUrlResolution = resolveModuleBase(
      `${prefix}/${specifier}`,
      modulePaths,
      "base_url"
    );
    if (baseUrlResolution.kind === "resolved" || baseUrlResolution.reason === "ConflictingFacts") {
      return baseUrlResolution;
    }
  }

  const aliasMatches = [];
  let matchedAlias = false;
  for (const alias of pathAliases(model)) {
    const replacements = aliasReplacements(specifier, alias.aliasPattern);
    if (!replacements) {
      continue;
    }
    matchedAlias = true;
    for (const replacement of replacements) {
      for (const targetPattern of alias.targetPatterns) {
        const resolution = resolveModuleBase(
          applyAliasTarget(targetPattern, replacement),
          modulePaths,
          "path_alias"
        );
        if (resolution.kind === "resolved") {
          aliasMatches.push(resolution.path);
        }
      }
    }
  }
  const uniqueAliasMatches = [...new Set(aliasMatches)].sort();
  if (uniqueAliasMatches.length === 1) {
    return { kind: "resolved", path: uniqueAliasMatches[0], resolutionKind: "path_alias" };
  }
  if (matchedAlias) {
    return uniqueAliasMatches.length > 1
      ? { kind: "unknown", reason: "ConflictingFacts", unknownKind: "path_alias_conflict" }
      : { kind: "unknown", reason: "UnresolvedImport", unknownKind: "unresolved_path_alias" };
  }

  const packageEntry = resolvePackageEntry(model, specifier);
  if (packageEntry.kind !== "ignored") {
    return packageEntry;
  }
  return { kind: "unknown", reason: "MissingDependency", unknownKind: "missing_dependency" };
}

function resolvePackageEntry(model, specifier) {
  if (!model.packageJson || typeof model.packageJson !== "object") {
    return { kind: "ignored" };
  }
  const packageName = model.packageJson.name;
  if (!isNonBlankString(packageName)) {
    return { kind: "ignored" };
  }
  if (specifier === packageName) {
    return resolveExplicitPackageTarget(model, ".");
  }
  if (specifier.startsWith(`${packageName}/`)) {
    return resolveExplicitPackageTarget(model, `./${specifier.slice(packageName.length + 1)}`);
  }
  if (specifier.startsWith("#")) {
    return resolveExplicitPackageTarget(model, specifier);
  }
  return { kind: "ignored" };
}

function resolveExplicitPackageTarget(model, key) {
  const packageJson = model.packageJson;
  const source =
    key.startsWith("#") && packageJson.imports
      ? packageJson.imports
      : packageJson.exports || {};
  const target = explicitPackageTarget(source, key);
  if (!target) {
    return { kind: "unknown", reason: "MissingDependency", unknownKind: "missing_package_entry" };
  }
  const normalized = normalizeConfigPath(target);
  if (!isSafeRepoRelativePath(normalized)) {
    return { kind: "unknown", reason: "ConflictingFacts", unknownKind: "unsafe_package_entry" };
  }
  return resolveModuleBase(normalized, new Set(model.modulePaths), "package_entry");
}

function explicitPackageTarget(value, key) {
  if (typeof value === "string" && key === ".") {
    return value;
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const entry = value[key];
  if (typeof entry === "string") {
    return entry;
  }
  if (entry && typeof entry === "object" && !Array.isArray(entry)) {
    for (const field of ["types", "import", "require", "default"]) {
      if (typeof entry[field] === "string") {
        return entry[field];
      }
    }
  }
  return null;
}

function exportedFactKind(text, exportName) {
  if (exportName === "default") {
    return /\bexport\s+default\b/.test(text) ? "SYMBOL" : null;
  }
  const escaped = exportName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (new RegExp(`\\bexport\\s+(?:interface|type)\\s+${escaped}\\b`).test(text)) {
    return "TYPE";
  }
  if (
    new RegExp(`\\bexport\\s+(?:const|let|var|function|class|enum)\\s+${escaped}\\b`).test(text) ||
    new RegExp(`\\bexport\\s*\\{[^}]*\\b${escaped}\\b[^}]*\\}`).test(text)
  ) {
    return "SYMBOL";
  }
  return null;
}

function exportedFactKindWithTypeScript(ts, text, exportName, fileName) {
  if (!ts || typeof ts.createSourceFile !== "function") {
    return null;
  }
  let sourceFile;
  try {
    sourceFile = ts.createSourceFile(
      fileName,
      text,
      ts.ScriptTarget && ts.ScriptTarget.Latest ? ts.ScriptTarget.Latest : 99,
      false
    );
  } catch (_error) {
    return null;
  }
  if (!sourceFile || !Array.isArray(sourceFile.statements)) {
    return null;
  }
  for (const statement of sourceFile.statements) {
    const factKind = exportedStatementFactKind(ts, statement, exportName);
    if (factKind) {
      return factKind;
    }
  }
  return null;
}

function exportedStatementFactKind(ts, statement, exportName) {
  if (exportName === "default") {
    if (hasModifier(ts, statement, "ExportKeyword") && hasModifier(ts, statement, "DefaultKeyword")) {
      return "SYMBOL";
    }
    if (typeof ts.isExportAssignment === "function" && ts.isExportAssignment(statement)) {
      return "SYMBOL";
    }
    return null;
  }
  if (
    (typeof ts.isInterfaceDeclaration === "function" && ts.isInterfaceDeclaration(statement)) ||
    (typeof ts.isTypeAliasDeclaration === "function" && ts.isTypeAliasDeclaration(statement))
  ) {
    return exportedDeclarationName(statement) === exportName && hasModifier(ts, statement, "ExportKeyword")
      ? "TYPE"
      : null;
  }
  if (
    (typeof ts.isFunctionDeclaration === "function" && ts.isFunctionDeclaration(statement)) ||
    (typeof ts.isClassDeclaration === "function" && ts.isClassDeclaration(statement)) ||
    (typeof ts.isEnumDeclaration === "function" && ts.isEnumDeclaration(statement))
  ) {
    return exportedDeclarationName(statement) === exportName && hasModifier(ts, statement, "ExportKeyword")
      ? "SYMBOL"
      : null;
  }
  if (typeof ts.isVariableStatement === "function" && ts.isVariableStatement(statement)) {
    if (!hasModifier(ts, statement, "ExportKeyword")) {
      return null;
    }
    const declarations = statement.declarationList && statement.declarationList.declarations;
    if (!Array.isArray(declarations)) {
      return null;
    }
    return declarations.some((declaration) => exportedDeclarationName(declaration) === exportName)
      ? "SYMBOL"
      : null;
  }
  if (typeof ts.isExportDeclaration === "function" && ts.isExportDeclaration(statement)) {
    return exportedDeclarationListContains(ts, statement, exportName);
  }
  return null;
}

function exportedDeclarationName(node) {
  return node && node.name && typeof node.name.text === "string" ? node.name.text : null;
}

function hasModifier(ts, node, kindName) {
  const kind = ts.SyntaxKind && ts.SyntaxKind[kindName];
  if (typeof kind !== "number") {
    return false;
  }
  const modifiers = typeof ts.getModifiers === "function" ? ts.getModifiers(node) : node.modifiers;
  return Array.isArray(modifiers) && modifiers.some((modifier) => modifier.kind === kind);
}

function exportedDeclarationListContains(ts, statement, exportName) {
  const clause = statement.exportClause;
  if (!clause || typeof ts.isNamedExports !== "function" || !ts.isNamedExports(clause)) {
    return null;
  }
  for (const element of clause.elements || []) {
    const name = element.name && element.name.text;
    if (name === exportName) {
      return statement.isTypeOnly ? "TYPE" : "SYMBOL";
    }
  }
  return null;
}

function splitReexportSpecifier(value) {
  const index = value.lastIndexOf("#");
  if (index === -1) {
    return { specifier: value, exportName: "default" };
  }
  return { specifier: value.slice(0, index), exportName: value.slice(index + 1) };
}

function origin(model, providerResolved) {
  if (providerResolved && model.typescript) {
    return {
      engine: "typescript",
      engine_version: model.typescript.version,
      method: "compiler_api_module_resolver_v1",
    };
  }
  return {
    engine: "repogrammar-tsjs-static-worker",
    engine_version: "0.1.0",
    method: "bounded_project_model_resolver_v1",
  };
}

function assumptions(model, operation, providerResolved, extra = []) {
  const providerAssumptions = providerResolved && model.typescript
    ? [
        "provider=typescript",
        "provider_resolved=true",
        "environment_fingerprint=node_typescript_compiler_api_v1",
      ]
    : [
        "provider=repogrammar_static_tsjs",
        "provider_resolved=false",
        "environment_fingerprint=node_static_worker_v1",
      ];
  return [
    ...providerAssumptions,
    `operation_id=${operation.operation_id}`,
    `query_operation=${operation.operation}`,
    `tsconfig_hash=${operation.project_config_hash}`,
    `package_json_hash=${operation.package_json_hash}`,
    ...extra,
  ];
}

function emitFact(
  requestId,
  model,
  operation,
  factKind,
  target,
  note,
  extra = [],
  options = {}
) {
  const providerResolved = options.providerResolved === true;
  message({
    protocol_version: PROTOCOL_VERSION,
    message_type: "fact",
    request_id: requestId,
    fact_kind: factKind,
    subject: `${operation.path}#${operation.operation}:${operation.start_byte}-${operation.end_byte}`,
    target,
    origin: origin(model, providerResolved),
    certainty: factKind === "UNKNOWN" ? "UNKNOWN" : providerResolved ? "SEMANTIC" : "STRUCTURAL",
    evidence: {
      code_unit_id: operation.code_unit_id,
      path: operation.path,
      content_hash: operation.content_hash,
      repository_revision: "UNKNOWN",
      start_byte: operation.start_byte,
      end_byte: operation.end_byte,
      note,
    },
    assumptions: assumptions(model, operation, providerResolved, extra),
  });
}

function emitResolvedImport(requestId, model, operation, resolution) {
  const providerResolved = resolution.provider === "compiler";
  emitFact(
    requestId,
    model,
    operation,
    "RESOLVED_IMPORT",
    `module:${resolution.path}`,
    providerResolved
      ? "TypeScript compiler resolved TS/JS module target"
      : "bounded project model resolved TS/JS module target",
    [`tsjs_import_resolution=${resolution.resolutionKind}`],
    { providerResolved }
  );
}

function emitUnknown(requestId, model, operation, reason, affectedClaim, unknownKind, note) {
  emitFact(requestId, model, operation, "UNKNOWN", reason, note, [
    `affected_claim=${affectedClaim}`,
    `tsjs_unknown_kind=${unknownKind}`,
  ]);
}

function runOperation(requestId, payload, model, operation) {
  if (operation.literal_specifier === "<dynamic>") {
    emitUnknown(
      requestId,
      model,
      operation,
      "DynamicImport",
      "tsjs_import_resolution",
      "dynamic_import",
      "dynamic TS/JS import was not resolved"
    );
    return;
  }
  if (operation.operation === "resolve_module_specifier") {
    const resolution = resolveSpecifier(model, operation.path, operation.literal_specifier);
    if (resolution.kind === "resolved") {
      emitResolvedImport(requestId, model, operation, resolution);
    } else {
      emitUnknown(
        requestId,
        model,
        operation,
        resolution.reason,
        "tsjs_import_resolution",
        resolution.unknownKind,
        "compiler could not prove a unique TS/JS module target"
      );
    }
    return;
  }
  if (operation.operation === "resolve_package_entry") {
    const resolution = resolvePackageEntry(model, operation.literal_specifier);
    if (resolution.kind === "resolved") {
      emitResolvedImport(requestId, model, operation, resolution);
    } else {
      emitUnknown(
        requestId,
        model,
        operation,
        resolution.reason || "MissingDependency",
        "tsjs_package_entry",
        resolution.unknownKind || "missing_package_entry",
        "compiler could not prove a TS/JS package entry"
      );
    }
    return;
  }
  if (operation.operation === "resolve_export") {
    const text = readBoundedText(payload.project_root, operation.path, operation.max_bytes);
    const providerFactKind = text
      ? exportedFactKindWithTypeScript(
          model.typescript,
          text,
          operation.literal_specifier,
          operation.path
        )
      : null;
    const factKind = providerFactKind || (text ? exportedFactKind(text, operation.literal_specifier) : null);
    if (factKind) {
      emitFact(
        requestId,
        model,
        operation,
        factKind,
        `symbol:${operation.path}#export:${operation.literal_specifier}`,
        providerFactKind
          ? "TypeScript compiler resolved TS/JS export symbol"
          : "bounded project model resolved TS/JS export symbol",
        [`tsjs_export_name=${operation.literal_specifier}`],
        { providerResolved: Boolean(providerFactKind) }
      );
    } else {
      emitUnknown(
        requestId,
        model,
        operation,
        "UnresolvedImport",
        "tsjs_export_resolution",
        "unresolved_export",
        "compiler could not prove a TS/JS export symbol"
      );
    }
    return;
  }
  if (operation.operation === "resolve_reexport") {
    if (operation.literal_specifier.endsWith("#*")) {
      emitUnknown(
        requestId,
        model,
        operation,
        "ConflictingFacts",
        "tsjs_reexport_resolution",
        "ambiguous_reexport",
        "star re-export is ambiguous without a unique exported symbol"
      );
      return;
    }
    const { specifier, exportName } = splitReexportSpecifier(operation.literal_specifier);
    const resolution = resolveSpecifier(model, operation.path, specifier);
    if (resolution.kind !== "resolved") {
      emitUnknown(
        requestId,
        model,
        operation,
        resolution.reason,
        "tsjs_reexport_resolution",
        resolution.unknownKind,
        "compiler could not resolve TS/JS re-export target"
      );
      return;
    }
    const text = readBoundedText(payload.project_root, resolution.path, operation.max_bytes);
    const providerFactKind = resolution.provider === "compiler" && text
      ? exportedFactKindWithTypeScript(
          model.typescript,
          text,
          exportName,
          resolution.path
        )
      : null;
    const factKind = providerFactKind || (text ? exportedFactKind(text, exportName) : null);
    if (factKind) {
      emitFact(
        requestId,
        model,
        operation,
        factKind,
        `symbol:${resolution.path}#export:${exportName}`,
        providerFactKind
          ? "TypeScript compiler resolved TS/JS re-export symbol"
          : "bounded project model resolved TS/JS re-export symbol",
        [
          `tsjs_export_name=${exportName}`,
          `tsjs_import_specifier=${specifier}`,
          `tsjs_import_resolution=${resolution.resolutionKind}`,
        ],
        { providerResolved: Boolean(providerFactKind) }
      );
    } else {
      emitUnknown(
        requestId,
        model,
        operation,
        "UnresolvedImport",
        "tsjs_reexport_resolution",
        "unresolved_reexport",
        "compiler could not prove a TS/JS re-export symbol"
      );
    }
  }
}

function main() {
  let payload;
  try {
    payload = JSON.parse(readStdin());
  } catch (_error) {
    emitWorkerError(
      DEFAULT_REQUEST_ID,
      "SEMANTIC_PROTOCOL_VIOLATION",
      "semantic worker request is invalid"
    );
    return;
  }

  const requestId = requestIdFor(payload);
  if (!validateRequest(payload)) {
    emitWorkerError(
      requestId,
      "SEMANTIC_PROTOCOL_VIOLATION",
      "semantic worker request is invalid"
    );
    return;
  }

  const model = loadProjectModel(payload);
  model.typescript = loadTypeScript(payload.project_root);
  for (const operation of payload.operations) {
    runOperation(requestId, payload, model, operation);
  }
  emitEnd(requestId);
}

main();
