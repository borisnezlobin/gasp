// A small TOML reader for Gasp's own default files: tables, arrays of
// tables, strings, numbers, booleans, arrays and inline tables. It is not a
// general TOML parser; it reads what crates/*/defaults/ use.

const BARE_KEY = /[A-Za-z0-9_-]/;
const ESCAPES = { n: "\n", t: "\t", '"': '"', "\\": "\\", r: "\r" };

function cursor(text) {
  return { text, at: 0 };
}

const peek = (c) => c.text[c.at];
const done = (c) => c.at >= c.text.length;

function skipSpace(c) {
  while (!done(c) && (peek(c) === " " || peek(c) === "\t")) c.at += 1;
}

function skipComment(c) {
  if (peek(c) !== "#") return;
  while (!done(c) && peek(c) !== "\n") c.at += 1;
}

function skipBlank(c) {
  for (;;) {
    const start = c.at;
    skipSpace(c);
    skipComment(c);
    if (peek(c) === "\n" || peek(c) === "\r") c.at += 1;
    if (c.at === start) return;
  }
}

function readString(c) {
  c.at += 1;
  let out = "";
  while (!done(c) && peek(c) !== '"') {
    if (peek(c) === "\\") {
      c.at += 1;
      out += ESCAPES[peek(c)] ?? peek(c);
    } else {
      out += peek(c);
    }
    c.at += 1;
  }
  c.at += 1;
  return out;
}

function readBareKey(c) {
  let out = "";
  while (!done(c) && BARE_KEY.test(peek(c))) {
    out += peek(c);
    c.at += 1;
  }
  return out;
}

function readKeyPart(c) {
  skipSpace(c);
  return peek(c) === '"' ? readString(c) : readBareKey(c);
}

function readKey(c) {
  const parts = [readKeyPart(c)];
  skipSpace(c);
  while (peek(c) === ".") {
    c.at += 1;
    parts.push(readKeyPart(c));
    skipSpace(c);
  }
  return parts;
}

function readArray(c) {
  c.at += 1;
  const items = [];
  for (;;) {
    skipBlank(c);
    if (peek(c) === "]") break;
    items.push(readValue(c));
    skipBlank(c);
    if (peek(c) === ",") c.at += 1;
  }
  c.at += 1;
  return items;
}

function readInlineTable(c) {
  c.at += 1;
  const table = {};
  for (;;) {
    skipSpace(c);
    if (peek(c) === "}") break;
    const key = readKey(c);
    c.at += 1;
    skipSpace(c);
    assign(table, key, readValue(c));
    skipSpace(c);
    if (peek(c) === ",") c.at += 1;
  }
  c.at += 1;
  return table;
}

function readScalar(c) {
  let raw = "";
  while (!done(c) && !/[\s,\]}#]/.test(peek(c))) {
    raw += peek(c);
    c.at += 1;
  }
  if (raw === "true") return true;
  if (raw === "false") return false;
  const number = Number(raw);
  if (raw !== "" && Number.isFinite(number)) return number;
  throw new Error(`Unreadable TOML value "${raw}" at ${c.at}`);
}

const VALUE_READERS = { '"': readString, "[": readArray, "{": readInlineTable };

function readValue(c) {
  skipSpace(c);
  const reader = VALUE_READERS[peek(c)] ?? readScalar;
  return reader(c);
}

function assign(table, path, value) {
  const parent = path.slice(0, -1).reduce((node, part) => (node[part] ??= {}), table);
  parent[path.at(-1)] = value;
}

function openTable(root, path) {
  return path.reduce((node, part) => (node[part] ??= {}), root);
}

function openArrayTable(root, path) {
  const parent = openTable(root, path.slice(0, -1));
  const list = (parent[path.at(-1)] ??= []);
  const entry = {};
  list.push(entry);
  return entry;
}

function readHeader(c, root) {
  const isArray = c.text.startsWith("[[", c.at);
  c.at += isArray ? 2 : 1;
  const path = readKey(c);
  c.at += isArray ? 2 : 1;
  return isArray ? openArrayTable(root, path) : openTable(root, path);
}

export function parseToml(text) {
  const c = cursor(text);
  const root = {};
  let table = root;
  for (skipBlank(c); !done(c); skipBlank(c)) {
    if (peek(c) === "[") {
      table = readHeader(c, root);
      continue;
    }
    const key = readKey(c);
    skipSpace(c);
    c.at += 1;
    assign(table, key, readValue(c));
  }
  return root;
}

/** Nested tables as dotted keys, such as `color.code.comment`. Arrays stay whole. */
export function flatten(table, prefix = "") {
  return Object.entries(table).flatMap(([key, value]) => {
    const name = prefix ? `${prefix}.${key}` : key;
    const isTable = value !== null && typeof value === "object" && !Array.isArray(value);
    return isTable && Object.keys(value).length > 0 ? flatten(value, name) : [[name, value]];
  });
}
