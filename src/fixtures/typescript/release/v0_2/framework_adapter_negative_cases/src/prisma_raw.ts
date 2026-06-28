import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

export async function rawUsers() {
  return prisma.$queryRaw("SELECT * FROM users");
}

export async function rawTransaction() {
  return prisma.$transaction([prisma.$executeRaw("DELETE FROM users")]);
}
