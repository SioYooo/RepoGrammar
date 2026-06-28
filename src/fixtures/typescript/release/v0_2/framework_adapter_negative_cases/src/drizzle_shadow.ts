import { drizzle } from "drizzle-orm/node-postgres";

const db = drizzle(pool);

export const listUsers = (db: unknown) => {
  return db.select().from(users);
};
