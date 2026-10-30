import type { Front } from "../lib/platform.ts";
import { thisMachine } from "../lib/words.ts";

// Diagnostics before it has any checks, as present/words/checks.rs `checking` says it.
export const checking = (front: Front) => `Checking ${thisMachine(front)}…`;
