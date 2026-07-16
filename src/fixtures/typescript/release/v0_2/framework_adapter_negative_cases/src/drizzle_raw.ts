import { drizzle, sql } from "drizzle-orm/node-postgres";
import { pgTable } from "drizzle-orm/pg-core";

export const users = pgTable("users", {});
const db = drizzle(pool);

export async function unsafeQuery() {
  return db.select({ unsafe: sql`raw` }).from(users);
}
