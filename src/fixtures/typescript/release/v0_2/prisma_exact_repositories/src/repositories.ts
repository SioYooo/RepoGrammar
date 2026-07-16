import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

export async function listUsers() {
  return prisma.user.findMany({ where: { active: true }, select: { id: true } });
}

export async function listAccounts() {
  return prisma.account.findMany({ where: { active: true }, select: { id: true } });
}

export async function listOrders() {
  return prisma.order.findMany({ where: { active: true }, select: { id: true } });
}

export async function savePair() {
  return prisma.$transaction([
    prisma.user.create({ data: { name: "Ada" } }),
    prisma.account.create({ data: { name: "Research" } }),
  ]);
}
