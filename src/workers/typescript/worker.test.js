"use strict";

const assert = require("assert");
const path = require("path");
const { spawnSync } = require("child_process");

const workerPath = path.join(__dirname, "worker.js");

function runWorker(payload) {
  const input =
    typeof payload === "string" ? payload : `${JSON.stringify(payload)}\n`;
  const result = spawnSync(process.execPath, [workerPath], {
    input,
    encoding: "utf8",
  });

  assert.strictEqual(result.status, 0, result.stderr);
  assert.strictEqual(result.stderr, "");
  return result.stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function validRequest() {
  return {
    protocol_version: 1,
    request_id: "repogrammar-typescript-semantic-worker",
    project_root: "/repo",
    changed_files: ["src/a.ts", "src/b.tsx"],
  };
}

function assertEndOfStream(messages, requestId) {
  assert.deepStrictEqual(messages.at(-1), {
    protocol_version: 1,
    message_type: "end_of_stream",
    request_id: requestId,
  });
}

{
  const messages = runWorker(validRequest());
  assert.strictEqual(messages.length, 2);
  assert.strictEqual(messages[0].message_type, "worker_error");
  assert.strictEqual(messages[0].error_code, "SEMANTIC_WORKER_UNAVAILABLE");
  assert.deepStrictEqual(messages[0].fallback, {
    mode: "syntax_only",
    certainty: "UNKNOWN",
  });
  assertEndOfStream(messages, "repogrammar-typescript-semantic-worker");
  assert(!JSON.stringify(messages).includes("/repo"));
  assert(!JSON.stringify(messages).includes("src/a.ts"));
}

{
  const request = validRequest();
  request.changed_files = Array.from(
    { length: 10_000 },
    (_, index) => `src/file-${String(index).padStart(5, "0")}.ts`
  );
  const requestBytes = Buffer.byteLength(`${JSON.stringify(request)}\n`, "utf8");
  assert(requestBytes > 4 * 1024);
  assert(requestBytes <= 1_048_576);
  const messages = runWorker(request);
  assert.strictEqual(messages[0].message_type, "worker_error");
  assert.strictEqual(messages[0].error_code, "SEMANTIC_WORKER_UNAVAILABLE");
  assertEndOfStream(messages, "repogrammar-typescript-semantic-worker");
}

{
  const messages = runWorker("{not-json}\n");
  assert.strictEqual(messages[0].message_type, "worker_error");
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  assertEndOfStream(messages, "repogrammar-typescript-semantic-worker");
  assert(!JSON.stringify(messages).includes("not-json"));
}

for (const changedFiles of [
  ["/tmp/secret.ts"],
  ["../secret.ts"],
  ["src/../secret.ts"],
  ["./src/a.ts"],
  ["src\\a.ts"],
  ["file:///tmp/secret.ts"],
  ["C:tmp/source.ts"],
  ["C:tmp"],
  ["D:repo/file.ts"],
  ["src/a.ts", "src/a.ts"],
]) {
  const request = validRequest();
  request.changed_files = changedFiles;
  const messages = runWorker(request);
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  assertEndOfStream(messages, "repogrammar-typescript-semantic-worker");
  const serialized = JSON.stringify(messages);
  assert(!serialized.includes("/tmp/secret"));
  assert(!serialized.includes("../secret"));
  assert(!serialized.includes("src/a.ts"));
}

for (const mutate of [
  (request) => {
    request.protocol_version = 2;
  },
  (request) => {
    request.request_id = " ";
  },
  (request) => {
    request.request_id = "/tmp/secret";
  },
  (request) => {
    request.request_id = "file:///tmp/secret.ts";
  },
  (request) => {
    request.request_id = "const secret = true;";
  },
  (request) => {
    request.project_root = "relative";
  },
  (request) => {
    request.changed_files = null;
  },
  (request) => {
    request.extra = true;
  },
]) {
  const request = validRequest();
  mutate(request);
  const messages = runWorker(request);
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  assertEndOfStream(messages, "repogrammar-typescript-semantic-worker");
  assert(!JSON.stringify(messages).includes("/tmp/secret"));
  assert(!JSON.stringify(messages).includes("const secret"));
}

{
  const messages = runWorker("x".repeat(1_048_577));
  assert.strictEqual(messages[0].error_code, "SEMANTIC_PROTOCOL_VIOLATION");
  assertEndOfStream(messages, "repogrammar-typescript-semantic-worker");
}
