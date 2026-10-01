// Reads the parts of Gasp's Rust source the site's schema needs: serde
// structs and enums, the command registry, and key names. Regular
// expressions are enough because the source follows one style.

export const kebab = (name) => name.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
export const snakeToKebab = (name) => name.replaceAll("_", "-");

function docOf(lines) {
  const doc = lines
    .map((line) => line.trim())
    .filter((line) => line.startsWith("///"))
    .map((line) => line.replace(/^\/\/\/\s?/, ""))
    .join(" ")
    .trim();
  return doc || undefined;
}

function bodyAfter(source, start) {
  let depth = 0;
  for (let index = source.indexOf("{", start); index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(source.indexOf("{", start) + 1, index);
  }
  return "";
}

function fieldsOf(body) {
  const fields = [];
  let pending = [];
  for (const line of body.split("\n")) {
    const field = line.match(/^\s*pub (\w+): (.+),\s*$/);
    if (field && !pending.some((each) => each.includes("schemars(skip)"))) {
      fields.push({ name: snakeToKebab(field[1]), type: field[2].trim(), doc: docOf(pending) });
    }
    pending = field || !line.trim() ? [] : [...pending, line];
  }
  return fields;
}

/** Every `pub struct Name { pub field: Type }` in the source, by name. */
export function structsIn(source) {
  const structs = {};
  for (const match of source.matchAll(/pub struct (\w+) \{/g)) {
    structs[match[1]] = fieldsOf(bodyAfter(source, match.index));
  }
  return structs;
}

function variantsOf(body) {
  return body
    .split("\n")
    .map((line) => line.match(/^\s*([A-Z]\w*),\s*$/)?.[1])
    .filter(Boolean)
    .map(kebab);
}

/** Every unit-variant `pub enum Name { A, B }` in the source, by name, in kebab-case. */
export function enumsIn(source) {
  const enums = {};
  for (const match of source.matchAll(/pub enum (\w+) \{/g)) {
    const variants = variantsOf(bodyAfter(source, match.index));
    if (variants.length > 0) enums[match[1]] = variants;
  }
  return enums;
}

const SPEC_CALL =
  /(spec|key_only)\(\s*"([^"]+)",\s*"([^"]+)",\s*"([^"]+)",?\s*\)\s*(?:\.icon\(\s*"([^"]+)"\s*\))?/g;

/** Every command `spec(...)` and `key_only(...)` declares, with its icon. */
export function commandsIn(source) {
  const seen = new Map();
  for (const [, kind, id, title, category, icon] of source.matchAll(SPEC_CALL)) {
    if (seen.has(id)) continue;
    seen.set(id, { id, title, category, icon: icon ?? "lightning", palette: kind === "spec" });
  }
  return [...seen.values()];
}

/** The names in a `const NAME: &[(&str, ...)]` lookup's first column. */
export function lookupNames(source, constName) {
  const start = source.indexOf(`const ${constName}`);
  if (start < 0) return [];
  const body = source.slice(start, source.indexOf("];", start));
  return [...body.matchAll(/\(\s*"([^"]+)"/g)].map((match) => match[1]);
}

/** The chords a platform keeps for itself, from keymap.rs's RESERVED table. */
export function reservedChords(source, platform) {
  const start = source.indexOf(`Platform::${platform},`);
  if (start < 0) return [];
  const body = source.slice(start, source.indexOf("],", start));
  return [...body.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
}
