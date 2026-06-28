import fastify from "fastify";

const app = fastify();

app.get("/users", async (request, reply) => reply.send([]));
app.post("/accounts", async (request, reply) => reply.send({ ok: true }));
app.delete("/orders/:id", async (request, reply) => reply.send({ ok: true }));

app.route({ method: "GET", url: "/reports", handler: async (request, reply) => reply.send([]) });
app.route({ method: "POST", url: "/reports", handler: async (request, reply) => reply.send({ ok: true }) });
app.route({ method: "PATCH", path: "/reports/:id", handler: async (request, reply) => reply.send({ ok: true }) });
