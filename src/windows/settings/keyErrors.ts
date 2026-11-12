import type { TranslateError } from "../../lib/types";

export function keyErrorCopy(e: unknown): string {
  const err = e as TranslateError | string;
  if (err === "InvalidKey") return "That key didn’t work. Check it in Google Cloud Console.";
  if (err === "Offline") return "Couldn’t reach Google. Check your connection.";
  if (err === "QuotaExceeded") return "This key is out of quota.";
  if (err === "RateLimited") return "Groq is rate-limiting this key right now. Wait a moment and try again.";
  if (typeof err === "object" && err && "Other" in err) return `Something went wrong: ${err.Other}`;
  return "Something went wrong. Try again.";
}
