// Custom wrappers: `describe` and `it` are locally declared aliases over a
// project-specific runner, so neither call must be treated as a Jest/Vitest
// suite or test anchor.
const describe = makeSuiteRunner();
const it = makeCaseRunner();

describe("wrapped", () => {
  it("is a custom wrapper", () => {});
});
