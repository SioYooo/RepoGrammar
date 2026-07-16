const app = {
  get(_path: string, _handler: (...args: unknown[]) => unknown) {},
};

app.get("/users", function listUsers(
  _req: unknown,
  res: { json(value: unknown): void },
) {
  res.json({ users: [] });
});
