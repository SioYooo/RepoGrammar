import fastify from "fastify";

const app = fastify();

export const register = (app: unknown) => {
  app.get("/users", async () => []);
};
