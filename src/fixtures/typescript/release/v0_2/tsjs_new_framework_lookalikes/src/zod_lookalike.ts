// `z` is not imported from `zod`, so this is not a Zod schema anchor.
const z = { object: (shape: unknown) => shape };

export const NotASchema = z.object({ id: 1, name: "x" });
