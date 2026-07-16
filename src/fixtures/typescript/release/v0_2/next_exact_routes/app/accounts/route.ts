export async function GET(request: Request) {
  return Response.json({ route: "accounts", url: request.url });
}

export const POST = async (request: Request) => {
  return Response.json({ route: "accounts", created: true, url: request.url });
};
