import express from "express";

// Lookalike: `fakeApp` is a plain object literal, not an express() application,
// so `.get` must not be treated as an Express route handler.
const fakeApp = {
  get(path: string, handler: unknown) {
    return handler;
  },
};

fakeApp.get("/health", () => {});

// Dynamic receiver: the application is produced by a function call rather than a
// resolved binding, so the method call must stay UNKNOWN / unsupported.
function buildApp() {
  return express();
}

buildApp().post("/dynamic", (req, res) => {
  res.end();
});

// Dynamic method: exact-anchor support requires a literal direct route method.
const method = "get";
app[method]("/dynamic-method", (req, res) => {
  res.end();
});

// Reassigned receiver: even if the name started as an Express app, later writes
// make the binding unsafe for family support.
let unsafeApp = express();
unsafeApp = buildApp();
unsafeApp.get("/unsafe", (req, res) => {
  res.end();
});
