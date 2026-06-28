import express from "express";

const app = express();

app.get("/users", (req, res) => {
  res.json([]);
});

app.post("/users", (req, res) => {
  res.json({});
});

app.delete("/users/:id", (req, res) => {
  res.json({});
});
