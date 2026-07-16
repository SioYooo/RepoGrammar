export function handler(request: Request): Response {
  return new Response(`ok: ${request.method}`);
}
