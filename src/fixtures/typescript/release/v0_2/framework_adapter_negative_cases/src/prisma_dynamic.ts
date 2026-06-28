export async function listUsers(prisma: unknown) {
  return prisma.user.findMany();
}
