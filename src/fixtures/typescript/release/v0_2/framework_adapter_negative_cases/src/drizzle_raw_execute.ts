import { drizzle, sql } from "drizzle-orm/node-postgres";

const db = drizzle(pool);

export async function unsafeExecute() {
  return db.execute(sql.raw("SELECT * FROM users"));
}
