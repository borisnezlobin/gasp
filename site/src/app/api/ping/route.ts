import { MAX_PING_BYTES, parsePing } from "@/lib/ping";
import { countPing, hasStore } from "@/lib/tally";

export const dynamic = "force-dynamic";

const noContent = () => new Response(null, { status: 204 });
const refused = () => new Response(null, { status: 400 });

function readJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** Counts one install's daily ping. Nothing about the request is kept but
    the four fields, added to the day's tally. */
export async function POST(request: Request): Promise<Response> {
  const text = await request.text();
  if (text.length > MAX_PING_BYTES) return refused();
  const ping = parsePing(readJson(text));
  if (!ping) return refused();
  if (!hasStore()) return noContent();
  try {
    await countPing(ping);
    return noContent();
  } catch {
    return new Response(null, { status: 503 });
  }
}
