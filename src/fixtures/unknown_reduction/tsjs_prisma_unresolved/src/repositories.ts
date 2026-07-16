const prisma = getPrismaClient();

export async function listUsers() {
  return prisma.user.findMany({ where: { active: true } });
}
