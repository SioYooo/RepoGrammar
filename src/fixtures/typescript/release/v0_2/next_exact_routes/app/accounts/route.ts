export async function GET(request: Request) {
  return Response.json({ route: "accounts", url: request.url });
}
