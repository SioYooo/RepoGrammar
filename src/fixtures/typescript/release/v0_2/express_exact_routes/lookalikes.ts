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
