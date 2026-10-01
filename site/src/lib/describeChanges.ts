import type { ConfigPatch } from "./configPatch";
import { COMMANDS } from "./gaspSchema";
import { macChord } from "./keyChords";

const words = (key: string) => key.replaceAll(".", " ").replaceAll("-", " ");

type Describer = (patch: ConfigPatch) => string[];

const DESCRIBERS: Describer[] = [
  ({ theme }) => Object.entries(theme ?? {}).filter(([key]) => !key.startsWith("dark.")).map(([key, value]) => `${words(key)} ${value}`),
  ({ settings }) => Object.entries(settings ?? {}).map(([key, value]) => `${words(key)} ${value}`),
  ({ toolbars }) => Object.entries(toolbars ?? {}).map(([id, spec]) => (spec.enabled === false ? `no ${id} bar` : `a ${id} bar${spec.place ? ` at ${words(spec.place)}` : ""}`)),
  ({ keys }) => (keys ?? []).map(({ keys: chord, command }) => `${macChord(chord)} runs ${COMMANDS.get(command)?.title ?? command}`),
  ({ replacements }) => (replacements ?? []).map(({ from, to }) => `typing ${from} gives ${to}`),
];

/** The changes in words, for screen readers, such as "color accent
    #2f8f5b, appearance theme dark". */
export function describeChanges(patch: ConfigPatch): string {
  return DESCRIBERS.flatMap((describe) => describe(patch)).join(", ");
}
