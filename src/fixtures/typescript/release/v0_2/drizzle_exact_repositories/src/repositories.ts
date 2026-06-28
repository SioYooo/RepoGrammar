import { drizzle } from "drizzle-orm/node-postgres";
import { pgTable } from "drizzle-orm/pg-core";

export const users = pgTable("users", {});
export const accounts = pgTable("accounts", {});
export const orders = pgTable("orders", {});

const db = drizzle(pool);

export async function listUsers() {
  return db.select().from(users).where(eq(users.id, 1));
}

export async function listAccounts() {
  return db.select().from(accounts).where(eq(accounts.id, 1));
}

export async function listOrders() {
  return db.select().from(orders).where(eq(orders.id, 1));
}

export async function createUser(values) {
  return db.insert(users).values(values).returning();
}
