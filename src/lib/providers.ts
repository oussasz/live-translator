import type { Provider } from "./types";

export const PROVIDERS: { id: Provider; name: string; keyLabel: string; placeholder: string; help: string }[] = [
  {
    id: "groq",
    name: "Groq",
    keyLabel: "Groq API key",
    placeholder: "gsk_…",
    help: "Create a free key at console.groq.com under API Keys. The free plan allows 30 requests a minute and 1,000 a day; the app paces itself to stay inside it.",
  },
  {
    id: "google",
    name: "Google Translate",
    keyLabel: "Google API key",
    placeholder: "AIzaSy…",
    help: "Create a key in Google Cloud Console under APIs and Services, Credentials, with the Cloud Translation API enabled.",
  },
];

export const GROQ_MODELS: { id: string; label: string }[] = [
  { id: "openai/gpt-oss-20b", label: "GPT-OSS 20B — fastest" },
  { id: "openai/gpt-oss-120b", label: "GPT-OSS 120B — best quality" },
  { id: "llama-3.3-70b-versatile", label: "Llama 3.3 70B" },
  { id: "llama-3.1-8b-instant", label: "Llama 3.1 8B — lightest" },
];

export function providerInfo(id: Provider) {
  return PROVIDERS.find((p) => p.id === id)!;
}
