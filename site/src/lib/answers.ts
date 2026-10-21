import type { Question } from "@/components/Faq";

/** How a team shares notes in Gasp, asked on more than one page. */
export const TEAM_QUESTION: Question = {
  question: "Can my team share notes in Gasp?",
  answer:
    "Yes, through GitHub, a website that stores files for you. First, one person makes a private folder for the notes on GitHub and invites the others. Then each person opens Gasp's sync settings and chooses Use a repository address and token. Gasp sends each person's changes to the others within a few minutes. You won't see other people type in real time, the way you do in Google Docs. If two people change the same line, Gasp shows both versions. You choose which one to keep.",
};
