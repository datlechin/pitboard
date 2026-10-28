// Windows' Diagnostics before it has any checks, as present/words/checks.rs `checking` says it.
export const checking = (system: "macos" | "windows") =>
  `Checking ${system === "macos" ? "this Mac" : "this PC"}…`;
