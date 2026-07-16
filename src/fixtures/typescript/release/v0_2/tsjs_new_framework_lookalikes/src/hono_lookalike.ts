// `router` does not trace to `new Hono()`, so no Hono route anchor forms.
const router = makeRouter();

router.get("/users", (c: { json(value: unknown): unknown }) => c.json([]));

declare function makeRouter(): {
  get(path: string, handler: (c: unknown) => unknown): void;
};
