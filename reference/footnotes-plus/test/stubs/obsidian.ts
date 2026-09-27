// Minimal stand-ins so the editor-facing modules can be imported and driven in
// Node during simulation. Only what insert.ts / jump.ts / apply.ts touch.

export class Notice {
  static messages: string[] = [];
  constructor(message: string) {
    Notice.messages.push(message);
  }
}

export class Editor {}
