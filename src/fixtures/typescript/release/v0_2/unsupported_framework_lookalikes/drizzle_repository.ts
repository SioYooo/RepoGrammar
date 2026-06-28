import { drizzle } from "drizzle-orm/node-postgres";

const db = drizzle({});

export async function listAccounts() {
  return db.select().from("accounts");
}
