const app = {
  get(_path: string, _handler: (...args: unknown[]) => unknown) {},
};

app.get("/accounts", function listAccounts(
  _req: unknown,
  res: { json(value: unknown): void },
) {
  res.json({ accounts: [] });
});
