import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

export async function listUsers(prisma: unknown) {
  return prisma.user.findMany();
}
