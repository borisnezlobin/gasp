export const PLATFORMS = ["mac", "ios", "linux", "windows"] as const;
export const ARCHES = ["arm64", "x86_64"] as const;

export type Platform = (typeof PLATFORMS)[number];
export type Arch = (typeof ARCHES)[number];

export type Ping = {
  version: string;
  platform: Platform;
  os: string;
  arch: Arch;
};

export const MAX_PING_BYTES = 512;

const PING_KEYS = ["version", "platform", "os", "arch"];
const VERSION_PATTERN = /^\d{1,3}\.\d{1,3}\.\d{1,3}(-[0-9A-Za-z.]{1,16})?$/;
const OS_PATTERN = /^\d{1,3}(\.\d{1,3}){0,2}$/;

function isOneOf<T extends string>(options: readonly T[], value: unknown): value is T {
  return typeof value === "string" && (options as readonly string[]).includes(value);
}

function matches(pattern: RegExp, value: unknown): value is string {
  return typeof value === "string" && pattern.test(value);
}

function hasExactlyThePingKeys(body: object): boolean {
  const keys = Object.keys(body);
  return keys.length === PING_KEYS.length && keys.every((key) => PING_KEYS.includes(key));
}

/** The ping in `body`, or null when anything about it is off. */
export function parsePing(body: unknown): Ping | null {
  if (typeof body !== "object" || body === null || Array.isArray(body)) return null;
  if (!hasExactlyThePingKeys(body)) return null;
  const { version, platform, os, arch } = body as Record<string, unknown>;
  const valid =
    matches(VERSION_PATTERN, version) &&
    isOneOf(PLATFORMS, platform) &&
    matches(OS_PATTERN, os) &&
    isOneOf(ARCHES, arch);
  return valid ? { version, platform, os, arch } : null;
}

const FIELD_SEPARATOR = "|";

export function pingField(ping: Ping): string {
  return [ping.platform, ping.version, ping.os, ping.arch].join(FIELD_SEPARATOR);
}

export function fieldPing(field: string): Ping | null {
  const [platform, version, os, arch] = field.split(FIELD_SEPARATOR);
  return parsePing({ version, platform, os, arch });
}
