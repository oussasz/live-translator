import type { StripStatus, TranslateError } from "./types";

export const STATUS_COPY: Partial<Record<StripStatus, string>> = {
  offline: "Offline — will retry",
  key_error: "API key problem",
  cap: "Free limit reached",
  quota_error: "Google quota exceeded",
  rate_limited: "Groq rate limit — retrying",
  error: "Translation failed",
  paste_fallback: "Press Ctrl+V to paste",
  password: "Not available in password fields",
};

export function errorCopy(e: TranslateError): string {
  if (e === "Offline") return STATUS_COPY.offline!;
  if (e === "InvalidKey") return STATUS_COPY.key_error!;
  if (e === "QuotaExceeded") return STATUS_COPY.quota_error!;
  if (e === "RateLimited") return STATUS_COPY.rate_limited!;
  if (e === "CapReached") return STATUS_COPY.cap!;
  return STATUS_COPY.error!;
}
