#!/usr/bin/env node
"use strict";

const fs = require("fs");

const PROTOCOL_VERSION = 1;
const DEFAULT_REQUEST_ID = "repogrammar-typescript-semantic-worker";
const MAX_STDIN_BYTES = 1_048_576;
const MAX_PROJECT_ROOT_CHARS = 4096;
const MAX_CHANGED_FILES = 10_000;
const MAX_CHANGED_FILE_CHARS = 4096;

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
  return (
    value.includes("\0") ||
    value.includes("\n") ||
    value.includes("\r") ||
    value.includes("://")
  );
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

  return true;
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
  message({
    protocol_version: PROTOCOL_VERSION,
    message_type: "end_of_stream",
    request_id: requestId,
  });
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

  emitWorkerError(
    requestId,
    "SEMANTIC_WORKER_UNAVAILABLE",
    "TypeScript compiler semantic worker is not bundled in this bootstrap"
  );
}

main();
