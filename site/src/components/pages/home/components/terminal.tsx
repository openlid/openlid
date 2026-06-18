// Terminal was promoted to the shared ui/ layer (home + install both consume it).
// This re-export keeps the home page's relative imports working.
export { Terminal } from "@/components/ui/terminal";
export type { TermLine, TermLineKind } from "@/components/ui/terminal";
