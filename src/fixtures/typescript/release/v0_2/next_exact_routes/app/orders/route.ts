export async function GET(request: Request) {
  return Response.json({ route: "orders", url: request.url });
}
