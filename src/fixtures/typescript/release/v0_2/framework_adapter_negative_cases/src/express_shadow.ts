import express from "express";

const app = express();

export function register(app: unknown) {
  app.get("/users", (req, res) => res.json([]));
}
