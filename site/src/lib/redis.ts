import { Redis } from "@upstash/redis";

/** The names Vercel's Upstash integration gives its REST address and
    token: plain, or with the prefix chosen when it was connected. */
const URL_NAMES = ["KV_REST_API_URL", "UPSTASH_KV_REST_API_URL", "UPSTASH_REDIS_REST_URL"];
const TOKEN_NAMES = ["KV_REST_API_TOKEN", "UPSTASH_KV_REST_API_TOKEN", "UPSTASH_REDIS_REST_TOKEN"];

const firstSet = (names: string[]) => names.map((name) => process.env[name]).find(Boolean);

/** Redis from Vercel's Upstash integration, or null when no store is
    connected. */
export function redisFromEnv(): Redis | null {
  const url = firstSet(URL_NAMES);
  const token = firstSet(TOKEN_NAMES);
  if (!url || !token) return null;
  return new Redis({ url, token });
}
