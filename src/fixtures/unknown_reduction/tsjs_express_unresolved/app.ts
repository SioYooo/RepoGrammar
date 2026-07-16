import express from "express";

const app = createApp();

function listUsers(_req: unknown, res: { json(value: unknown): void }) {
  res.json([]);
}

app.get("/users", listUsers);
