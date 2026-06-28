export async function GET(request: Request) {
  return Response.json({ route: "users", url: request.url });
}
