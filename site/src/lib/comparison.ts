import type { Question } from "@/components/Faq";

/** Which app a reason to choose points to. */
export type Side = "gasp" | "rival";

/** How fully an app covers one row of the table. */
export type Coverage = "yes" | "partly" | "no";

export type Cell = { coverage: Coverage; text: string };

export type Row = { topic: string; gasp: Cell; rival: Cell };

/** A figure measured on both apps, the lower the better. */
export type Measure = { name: string; unit: string; gasp: number; rival: number; method: string };

export type Comparison = {
  slug: string;
  rival: string;
  /** The page's title tag and the description search results show. */
  title: string;
  description: string;
  /** The answer in one or two sentences, under the page's name. */
  summary: string;
  /** Reasons to pick each app, phrased as what the reader needs. */
  reasons: { gasp: string[]; rival: string[] };
  measures?: Measure[];
  rows: Row[];
  switching: { title: string; steps: string[] };
  questions: Question[];
};
