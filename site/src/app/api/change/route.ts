import { handleChange } from "@/lib/changeHandler";

/** Turns a visitor's plain-words request into changes to the demo window
    in the "Change anything" section. */
export async function POST(request: Request): Promise<Response> {
  return handleChange(request);
}
