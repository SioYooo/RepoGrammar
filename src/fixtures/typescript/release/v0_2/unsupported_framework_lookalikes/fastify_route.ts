import fastify from "fastify";

const app = fastify();
const method = "get";

app[method]("/users", async () => {
  return [];
});
