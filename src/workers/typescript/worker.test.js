"use strict";

const assert = require("assert");
const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const workerPath = path.join(__dirname, "worker.js");
const zeroHash =
  "sha256:0000000000000000000000000000000000000000000000000000000000000000";

function sha(text) {
  return `sha256:${crypto.createHash("sha256").update(text, "utf8").digest("hex")}`;
}

function workspace(name) {
  return fs.mkdtempSync(path.join(os.tmpdir(), `repogrammar-ts-worker-${name}-`));
}

function writeFile(root, relativePath, text) {
  const absolute = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(absolute), { recursive: true });
  fs.writeFileSync(absolute, text);
  return { path: relativePath, text, hash: sha(text) };
}

function runWorker(payload, extraEnv = {}) {
  const result = spawnSync(process.execPath, [workerPath], {
    input: `${JSON.stringify(payload)}\n`,
    encoding: "utf8",
    env: { ...process.env, ...extraEnv },
  });

  assert.strictEqual(result.status, 0, result.stderr);
  assert.strictEqual(result.stderr, "");
  return result.stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

// Loading the analyzed repo's TypeScript is arbitrary code execution, so it is
// opt-in. Compiler-API tests must explicitly trust the project fixture.
const TRUST_PROJECT_TS = { REPOGRAMMAR_TSJS_TRUST_PROJECT_TYPESCRIPT: "1" };

function request(root, files, operations) {
  return {
    protocol_version: 1,
    request_id: "repogrammar-typescript-semantic-worker",
    project_root: root,
    changed_files: files.map((file) => file.path).sort(),
    operations,
  };
}

function operation(file, literalSpecifier, overrides = {}) {
  const {
    projectConfigHash,
    packageJsonHash,
    project_config_hash,
    package_json_hash,
    ...rest
  } = overrides;
  return {
    operation_id: `op-${file.path.replace(/[^A-Za-z0-9]+/g, "-")}-${file.text.length}`,
    operation: "resolve_module_specifier",
    path: file.path,
    content_hash: file.hash,
    code_unit_id: `unit:${file.path}#module:0-${file.text.length}:0`,
    start_byte: 0,
    end_byte: file.text.length,
    literal_specifier: literalSpecifier,
    project_config_hash: projectConfigHash || project_config_hash || zeroHash,
    package_json_hash: packageJsonHash || package_json_hash || zeroHash,
    max_files: 100,
    max_bytes: 1_048_576,
    ...rest,
  };
}

function facts(messages) {
  assert.deepStrictEqual(messages.at(-1), {
    protocol_version: 1,
    message_type: "end_of_stream",
    request_id: "repogrammar-typescript-semantic-worker",
  });
  return messages.filter((message) => message.message_type === "fact");
}

function singleFact(messages) {
  const result = facts(messages);
  assert.strictEqual(result.length, 1, JSON.stringify(messages));
  return result[0];
}

function assertResolved(fact, target, resolutionKind) {
  assert.strictEqual(fact.fact_kind, "RESOLVED_IMPORT");
  assert.strictEqual(fact.target, target);
  assert.strictEqual(fact.origin.engine, "repogrammar-tsjs-static-worker");
  assert.strictEqual(fact.origin.method, "bounded_project_model_resolver_v1");
  assert.strictEqual(fact.certainty, "STRUCTURAL");
  assert(fact.assumptions.includes("provider=repogrammar_static_tsjs"));
  assert(fact.assumptions.includes("provider_resolved=false"));
  assert(fact.assumptions.includes(`tsjs_import_resolution=${resolutionKind}`));
  assert(!JSON.stringify(fact).includes("const "));
}

function assertCompilerResolved(fact, target) {
  assert.strictEqual(fact.fact_kind, "RESOLVED_IMPORT");
  assert.strictEqual(fact.target, target);
  assert.strictEqual(fact.origin.engine, "typescript");
  assert.strictEqual(fact.origin.engine_version, "6.0.0");
  assert.strictEqual(fact.origin.method, "compiler_api_module_resolver_v1");
  assert.strictEqual(fact.certainty, "SEMANTIC");
  assert(fact.assumptions.includes("provider=typescript"));
  assert(fact.assumptions.includes("provider_resolved=true"));
  assert(fact.assumptions.includes("tsjs_import_resolution=compiler_api"));
  assert(!JSON.stringify(fact).includes("const "));
}

function assertCompilerResolvedExport(fact, factKind, target, exportName) {
  assert.strictEqual(fact.fact_kind, factKind);
  assert.strictEqual(fact.target, target);
  assert.strictEqual(fact.origin.engine, "typescript");
  assert.strictEqual(fact.origin.engine_version, "6.0.0");
  assert.strictEqual(fact.origin.method, "compiler_api_module_resolver_v1");
  assert.strictEqual(fact.certainty, "SEMANTIC");
  assert(fact.assumptions.includes("provider=typescript"));
  assert(fact.assumptions.includes("provider_resolved=true"));
  assert(fact.assumptions.includes("query_operation=resolve_export"));
  assert(fact.assumptions.includes(`tsjs_export_name=${exportName}`));
  assert(!JSON.stringify(fact).includes("export async function"));
}

function assertCompilerResolvedReexport(fact, factKind, target, exportName, specifier) {
  assert.strictEqual(fact.fact_kind, factKind);
  assert.strictEqual(fact.target, target);
  assert.strictEqual(fact.origin.engine, "typescript");
  assert.strictEqual(fact.origin.engine_version, "6.0.0");
  assert.strictEqual(fact.origin.method, "compiler_api_module_resolver_v1");
  assert.strictEqual(fact.certainty, "SEMANTIC");
  assert(fact.assumptions.includes("provider=typescript"));
  assert(fact.assumptions.includes("provider_resolved=true"));
  assert(fact.assumptions.includes("query_operation=resolve_reexport"));
  assert(fact.assumptions.includes(`tsjs_export_name=${exportName}`));
  assert(fact.assumptions.includes(`tsjs_import_specifier=${specifier}`));
  assert(fact.assumptions.includes("tsjs_import_resolution=compiler_api"));
  assert(!JSON.stringify(fact).includes("new PrismaClient"));
}

function assertUnknown(fact, reason, kind) {
  assert.strictEqual(fact.fact_kind, "UNKNOWN");
  assert.strictEqual(fact.target, reason);
  assert.strictEqual(fact.certainty, "UNKNOWN");
  assert(fact.assumptions.includes(`tsjs_unknown_kind=${kind}`));
}

{
  const root = workspace("relative");
  const source = writeFile(root, "src/route.ts", "import handler from './handler';\n");
  writeFile(root, "src/handler.ts", "export default function handler() {}\n");

  const fact = singleFact(runWorker(request(root, [source, { path: "src/handler.ts" }], [
    operation(source, "./handler"),
  ])));

  assertResolved(fact, "module:src/handler.ts", "literal_relative");
}

{
  const root = workspace("compiler-api");
  const source = writeFile(root, "src/route.ts", "import handler from './handler';\n");
  writeFile(root, "src/handler.ts", "export default function handler() {}\n");
  writeFile(
    root,
    "node_modules/typescript/index.js",
    `
const path = require("path");
exports.version = "6.0.0";
exports.resolveModuleName = (specifier, containingFile) => ({
  resolvedModule: {
    resolvedFileName: path.join(path.dirname(containingFile), specifier + ".ts")
  }
});
`
  );

  const fact = singleFact(runWorker(request(root, [source, { path: "src/handler.ts" }], [
    operation(source, "./handler"),
  ]), TRUST_PROJECT_TS));

  assertCompilerResolved(fact, "module:src/handler.ts");
}

{
  // Security: by default the worker must NOT load the analyzed repo's own
  // `typescript` (arbitrary code execution). With a hostile project TypeScript
  // present but no trust opt-in, the worker falls back to structural
  // resolution and never runs the project package.
  const root = workspace("untrusted-project-typescript");
  const source = writeFile(root, "src/route.ts", "import handler from './handler';\n");
  writeFile(root, "src/handler.ts", "export default function handler() {}\n");
  writeFile(
    root,
    "node_modules/typescript/index.js",
    `require("fs").writeFileSync(${JSON.stringify(
      path.join(root, "PWNED")
    )}, "executed");\nexports.version = "6.0.0";\nexports.resolveModuleName = () => ({});\n`
  );

  const fact = singleFact(runWorker(request(root, [source, { path: "src/handler.ts" }], [
    operation(source, "./handler"),
  ])));

  assertResolved(fact, "module:src/handler.ts", "literal_relative");
  assert.strictEqual(
    fs.existsSync(path.join(root, "PWNED")),
    false,
    "worker must not execute the analyzed repo's typescript package"
  );
}

{
  const root = workspace("compiler-api-static-fallback");
  const source = writeFile(root, "src/route.ts", "import service from 'services/user';\n");
  const target = writeFile(root, "src/services/user.ts", "export default {};\n");
  const config = writeFile(root, "tsconfig.json", '{"compilerOptions":{"baseUrl":"src"}}');
  writeFile(
    root,
    "node_modules/typescript/index.js",
    `
exports.version = "6.0.0";
exports.resolveModuleName = () => ({});
`
  );

  const fact = singleFact(runWorker(request(root, [source, target, config], [
    operation(source, "services/user", { projectConfigHash: config.hash }),
  ]), TRUST_PROJECT_TS));

  assertResolved(fact, "module:src/services/user.ts", "base_url");
}

{
  const root = workspace("baseurl");
  const source = writeFile(root, "src/route.ts", "import service from 'services/user';\n");
  const target = writeFile(root, "src/services/user.ts", "export default {};\n");
  const config = writeFile(root, "tsconfig.json", '{"compilerOptions":{"baseUrl":"src"}}');

  const fact = singleFact(runWorker(request(root, [source, target, config], [
    operation(source, "services/user", { projectConfigHash: config.hash }),
  ])));

  assertResolved(fact, "module:src/services/user.ts", "base_url");
}

{
  const root = workspace("paths");
  const source = writeFile(root, "src/route.ts", "import service from '@app/service';\n");
  const target = writeFile(root, "src/app/service.ts", "export const service = true;\n");
  const config = writeFile(
    root,
    "tsconfig.json",
    '{"compilerOptions":{"baseUrl":"src","paths":{"@app/*":["app/*"]}}}'
  );

  const fact = singleFact(runWorker(request(root, [source, target, config], [
    operation(source, "@app/service", { projectConfigHash: config.hash }),
  ])));

  assertResolved(fact, "module:src/app/service.ts", "path_alias");
}

{
  const root = workspace("paths-multiple-wildcards");
  const source = writeFile(root, "src/route.ts", "import service from '@app/service';\n");
  const target = writeFile(root, "src/app/service/service.ts", "export const service = true;\n");
  const config = writeFile(
    root,
    "tsconfig.json",
    '{"compilerOptions":{"baseUrl":"src","paths":{"@app/*":["app/*/*"]}}}'
  );

  const fact = singleFact(runWorker(request(root, [source, target, config], [
    operation(source, "@app/service", { projectConfigHash: config.hash }),
  ])));

  assertUnknown(fact, "UnresolvedImport", "unresolved_path_alias");
}

{
  const root = workspace("rootdirs");
  const source = writeFile(root, "generated/route.ts", "import shared from './shared';\n");
  const target = writeFile(root, "src/shared.ts", "export default {};\n");
  const config = writeFile(
    root,
    "tsconfig.json",
    '{"compilerOptions":{"rootDirs":["src","generated"]}}'
  );

  const fact = singleFact(runWorker(request(root, [source, target, config], [
    operation(source, "./shared", { projectConfigHash: config.hash }),
  ])));

  assertResolved(fact, "module:src/shared.ts", "root_dirs");
}

{
  const root = workspace("exports");
  const source = writeFile(
    root,
    "src/exported.ts",
    "export default class User {}\nexport const named = true;\nexport interface UserDto {}\n"
  );
  const defaultFact = singleFact(runWorker(request(root, [source], [
    operation(source, "default", { operation: "resolve_export" }),
  ])));
  const namedFact = singleFact(runWorker(request(root, [source], [
    operation(source, "named", { operation: "resolve_export" }),
  ])));
  const typeFact = singleFact(runWorker(request(root, [source], [
    operation(source, "UserDto", { operation: "resolve_export" }),
  ])));

  assert.strictEqual(defaultFact.fact_kind, "SYMBOL");
  assert.strictEqual(defaultFact.target, "symbol:src/exported.ts#export:default");
  assert.strictEqual(namedFact.fact_kind, "SYMBOL");
  assert.strictEqual(namedFact.target, "symbol:src/exported.ts#export:named");
  assert.strictEqual(typeFact.fact_kind, "TYPE");
  assert.strictEqual(typeFact.target, "symbol:src/exported.ts#export:UserDto");
}

{
  const root = workspace("compiler-api-export");
  const source = writeFile(
    root,
    "src/route.ts",
    "export async function GET() {}\nexport default function Page() {}\n"
  );
  writeFile(
    root,
    "node_modules/typescript/index.js",
    `
exports.version = "6.0.0";
exports.resolveModuleName = () => ({});
exports.ScriptTarget = { Latest: 99 };
exports.SyntaxKind = { ExportKeyword: 1, DefaultKeyword: 2 };
exports.getModifiers = (node) => node.modifiers || [];
exports.createSourceFile = (_fileName, text) => ({
  statements: [
    ...(text.includes("function GET") ? [{
      kind: "function",
      name: { text: "GET" },
      modifiers: [{ kind: 1 }],
    }] : []),
    ...(text.includes("default function Page") ? [{
      kind: "function",
      name: { text: "Page" },
      modifiers: [{ kind: 1 }, { kind: 2 }],
    }] : []),
  ],
});
exports.isFunctionDeclaration = (node) => node.kind === "function";
exports.isClassDeclaration = () => false;
exports.isEnumDeclaration = () => false;
exports.isInterfaceDeclaration = () => false;
exports.isTypeAliasDeclaration = () => false;
exports.isVariableStatement = () => false;
exports.isExportAssignment = () => false;
exports.isExportDeclaration = () => false;
exports.isNamedExports = () => false;
`
  );

  const getFact = singleFact(runWorker(request(root, [source], [
    operation(source, "GET", { operation: "resolve_export" }),
  ]), TRUST_PROJECT_TS));
  const defaultFact = singleFact(runWorker(request(root, [source], [
    operation(source, "default", { operation: "resolve_export" }),
  ]), TRUST_PROJECT_TS));

  assertCompilerResolvedExport(getFact, "SYMBOL", "symbol:src/route.ts#export:GET", "GET");
  assertCompilerResolvedExport(
    defaultFact,
    "SYMBOL",
    "symbol:src/route.ts#export:default",
    "default"
  );
}

{
  const root = workspace("reexport");
  const source = writeFile(root, "src/index.ts", "export { named } from './exported';\n");
  const target = writeFile(root, "src/exported.ts", "export const named = true;\n");

  const fact = singleFact(runWorker(request(root, [source, target], [
    operation(source, "./exported#named", { operation: "resolve_reexport" }),
  ])));

  assert.strictEqual(fact.fact_kind, "SYMBOL");
  assert.strictEqual(fact.target, "symbol:src/exported.ts#export:named");
}

{
  const root = workspace("compiler-api-reexport");
  const source = writeFile(root, "src/repository.ts", "import { prisma } from './db';\n");
  const target = writeFile(root, "src/db.ts", "export const prisma = new PrismaClient();\n");
  const handlerTarget = writeFile(root, "src/handlers.ts", "export function listUsers() {}\n");
  writeFile(
    root,
    "node_modules/typescript/index.js",
    `
const path = require("path");
exports.version = "6.0.0";
exports.resolveModuleName = (specifier, containingFile) => ({
  resolvedModule: {
    resolvedFileName: path.join(path.dirname(containingFile), specifier + ".ts")
  }
});
exports.ScriptTarget = { Latest: 99 };
exports.SyntaxKind = { ExportKeyword: 1 };
exports.getModifiers = (node) => node.modifiers || [];
exports.createSourceFile = (_fileName, text) => ({
  statements: [
    ...(text.includes("const prisma") ? [{
      kind: "variable",
      modifiers: [{ kind: 1 }],
      declarationList: { declarations: [{ name: { text: "prisma" } }] },
    }] : []),
    ...(text.includes("function listUsers") ? [{
      kind: "function",
      name: { text: "listUsers" },
      modifiers: [{ kind: 1 }],
    }] : []),
  ],
});
exports.isFunctionDeclaration = (node) => node.kind === "function";
exports.isClassDeclaration = () => false;
exports.isEnumDeclaration = () => false;
exports.isInterfaceDeclaration = () => false;
exports.isTypeAliasDeclaration = () => false;
exports.isVariableStatement = (node) => node.kind === "variable";
exports.isExportAssignment = () => false;
exports.isExportDeclaration = () => false;
exports.isNamedExports = () => false;
`
  );

  const fact = singleFact(runWorker(request(root, [source, target, handlerTarget], [
    operation(source, "./db#prisma", { operation: "resolve_reexport" }),
  ]), TRUST_PROJECT_TS));
  const handlerFact = singleFact(runWorker(request(root, [source, target, handlerTarget], [
    operation(source, "./handlers#listUsers", { operation: "resolve_reexport" }),
  ]), TRUST_PROJECT_TS));

  assertCompilerResolvedReexport(
    fact,
    "SYMBOL",
    "symbol:src/db.ts#export:prisma",
    "prisma",
    "./db"
  );
  assertCompilerResolvedReexport(
    handlerFact,
    "SYMBOL",
    "symbol:src/handlers.ts#export:listUsers",
    "listUsers",
    "./handlers"
  );
}

{
  const root = workspace("barrel");
  const source = writeFile(root, "src/index.ts", "export * from './a';\nexport * from './b';\n");

  const fact = singleFact(runWorker(request(root, [source], [
    operation(source, "./a#*", { operation: "resolve_reexport" }),
  ])));

  assertUnknown(fact, "ConflictingFacts", "ambiguous_reexport");
}

{
  const root = workspace("package");
  const source = writeFile(root, "src/route.ts", "import feature from 'acme/feature';\n");
  const target = writeFile(root, "src/feature.ts", "export default {};\n");
  const packageJson = writeFile(
    root,
    "package.json",
    '{"name":"acme","exports":{"./feature":"./src/feature.ts"},"type":"module","dependencies":{"fastify":"latest"},"devDependencies":{"vitest":"latest"}}'
  );

  const fact = singleFact(runWorker(request(root, [source, target, packageJson], [
    operation(source, "acme/feature", {
      operation: "resolve_package_entry",
      packageJsonHash: packageJson.hash,
    }),
  ])));

  assertResolved(fact, "module:src/feature.ts", "package_entry");
}

{
  const root = workspace("commonjs");
  const source = writeFile(root, "src/cjs.ts", "const handler = require('./handler');\n");
  const target = writeFile(root, "src/handler.ts", "module.exports = function handler() {};\n");

  const fact = singleFact(runWorker(request(root, [source, target], [
    operation(source, "./handler"),
  ])));

  assertResolved(fact, "module:src/handler.ts", "literal_relative");
}

{
  const root = workspace("external");
  const source = writeFile(root, "src/route.ts", "import missing from 'left-pad';\n");

  const fact = singleFact(runWorker(request(root, [source], [
    operation(source, "left-pad"),
  ])));

  assertUnknown(fact, "MissingDependency", "missing_dependency");
}

{
  const root = workspace("malformed-config");
  const source = writeFile(root, "src/route.ts", "import service from 'services/user';\n");
  const config = writeFile(root, "tsconfig.json", '{"compilerOptions":');

  const fact = singleFact(runWorker(request(root, [source, config], [
    operation(source, "services/user", { projectConfigHash: config.hash }),
  ])));

  assertUnknown(fact, "MissingProjectConfig", "malformed_project_config");
}

{
  const root = workspace("dynamic");
  const source = writeFile(root, "src/route.ts", "import(name);\n");

  const fact = singleFact(runWorker(request(root, [source], [
    operation(source, "<dynamic>"),
  ])));

  assertUnknown(fact, "DynamicImport", "dynamic_import");
}

{
  const messages = runWorker("{not-json}\n");
  assert.strictEqual(messages[0].message_type, "worker_error");
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  assert(!JSON.stringify(messages).includes("not-json"));
}

for (const changedFiles of [
  ["/tmp/secret.ts"],
  ["../secret.ts"],
  ["src/../secret.ts"],
  ["./src/a.ts"],
  ["src\\a.ts"],
  ["src//a.ts"],
  ["src/\u0008/a.ts"],
  ["file:///tmp/secret.ts"],
  ["C:tmp/source.ts"],
  ["C:tmp"],
  ["D:repo/file.ts"],
  ["src/a.ts", "src/a.ts"],
]) {
  const root = workspace("invalid");
  const source = writeFile(root, "src/a.ts", "export const value = true;\n");
  const payload = request(root, [source], [operation(source, "./b")]);
  payload.changed_files = changedFiles;
  const messages = runWorker(payload);
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  const serialized = JSON.stringify(messages);
  assert(!serialized.includes("/tmp/secret"));
  assert(!serialized.includes("../secret"));
  assert(!serialized.includes("src/a.ts"));
}

{
  const root = workspace("invalid-operation");
  const source = writeFile(root, "src/a.ts", "export const value = true;\n");
  const payload = request(root, [source], [
    operation(source, "const secret = true;", { operation: "resolve_module_specifier" }),
  ]);
  const messages = runWorker(payload);
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  assert(!JSON.stringify(messages).includes("const secret"));
}
