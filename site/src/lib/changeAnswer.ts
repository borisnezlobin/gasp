import { builtinAnswer, isBuiltinId, type BuiltinId } from "./builtins";
import { settingsFiles, type SettingsFile } from "./configFiles";
import { hasChanges, parsePatch, type ConfigPatch, type Refusal } from "./configPatch";
import { macChord } from "./keyChords";

export type ChangeAnswer = { patch: ConfigPatch; files: SettingsFile[]; builtin?: BuiltinId; reply?: string };

export const MAX_REPLY_LENGTH = 140;
const NOTHING_TO_CHANGE = "Gasp's settings can't change that.";

/** A short plain-text line, or undefined when there's nothing usable. */
export function parseReply(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const line = value.replace(/[\u0000-\u001f\u007f<>]/g, " ").replace(/\s+/g, " ").trim();
  if (!line) return undefined;
  return line.length > MAX_REPLY_LENGTH ? `${line.slice(0, MAX_REPLY_LENGTH - 1).trimEnd()}…` : line;
}

const REFUSAL_LINES: Record<Refusal["kind"], (refusal: Refusal) => string> = {
  "reserved-chord": ({ chord }) => `macOS keeps ${macChord(chord)} for itself, so Gasp can't use it.`,
};

/** What the visitor reads: a built-in's answer, why something was set
    aside, or the model's own line when part of the request can't be done. */
function replyFor(modelJson: unknown, patch: ConfigPatch, refusals: Refusal[], builtin: BuiltinId | undefined): string | undefined {
  const lines = [
    builtin ? builtinAnswer(builtin) : undefined,
    ...refusals.map((refusal) => REFUSAL_LINES[refusal.kind](refusal)),
    builtin ? undefined : parseReply((modelJson as { reply?: unknown })?.reply),
  ].filter((line): line is string => line !== undefined);
  if (lines.length === 0 && !hasChanges(patch)) return NOTHING_TO_CHANGE;
  return lines.length ? lines.join(" ") : undefined;
}

/** Builds the answer from the model's JSON: only what Gasp's schema allows,
    and files derived from that alone. */
export function answerFrom(modelJson: unknown): ChangeAnswer {
  const { patch, refusals } = parsePatch(modelJson);
  const named = (modelJson as { builtin?: unknown })?.builtin;
  const builtin = isBuiltinId(named) ? named : undefined;
  const answer: ChangeAnswer = { patch, files: settingsFiles(patch) };
  if (builtin) answer.builtin = builtin;
  const reply = replyFor(modelJson, patch, refusals, builtin);
  return reply ? { ...answer, reply } : answer;
}
