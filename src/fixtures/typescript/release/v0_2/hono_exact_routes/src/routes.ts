import { Hono } from "hono";

const app = new Hono();

app.get("/users", (c) => c.json([]));
app.get("/accounts", (c) => c.json([]));
app.get("/orders", (c) => c.json([]));
