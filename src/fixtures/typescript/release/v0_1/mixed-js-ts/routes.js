const app = {
  get(_path, _handler) {},
};

app.get("/health", function health(_req, res) {
  res.json({ ok: true });
});
