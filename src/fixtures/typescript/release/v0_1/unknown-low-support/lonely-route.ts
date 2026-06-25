const app = {
  get(_path: string, _handler: (...args: unknown[]) => unknown) {},
};

app.get("/lonely", function lonelyRoute(
  _req: unknown,
  res: { json(value: unknown): void },
) {
  res.json({ ok: true });
});
