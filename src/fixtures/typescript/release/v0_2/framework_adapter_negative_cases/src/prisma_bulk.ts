import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

export async function bulkUsers() {
  return prisma.user.createMany({ data: [] });
}
