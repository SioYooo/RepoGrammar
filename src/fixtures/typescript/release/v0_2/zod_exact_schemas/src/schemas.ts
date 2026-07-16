import { z } from "zod";

export const User = z.object({ id: z.string(), name: z.string() });
export const Account = z.object({ id: z.string(), balance: z.number() });
export const Order = z.object({ id: z.string(), total: z.number(), userId: z.string() });
