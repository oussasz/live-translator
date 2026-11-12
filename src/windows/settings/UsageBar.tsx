import type { Provider, UsageSnapshot } from "../../lib/types";

export function UsageBar({ usage, provider }: { usage: UsageSnapshot; provider: Provider }) {
  const groq = provider === "groq";
  const used = groq ? usage.requests_today : usage.chars;
  const limit = groq ? usage.request_limit : usage.limit;
  const pct = Math.min(100, Math.round((used / limit) * 100));
  const fmt = (n: number) => n.toLocaleString("en-US");
  const text = groq ? `Today: ${fmt(used)} / ${fmt(limit)} requests` : `This month: ${fmt(used)} / ${fmt(limit)} free`;
  return (
    <div className="field">
      <div className="row">
        <span className="row-label">{text}</span>
        <span className="hint">{pct}%</span>
      </div>
      <div
        className="usage-track"
        data-testid="usage-bar"
        data-warn={pct >= 80 ? "true" : "false"}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={limit}
        aria-valuenow={used}
        aria-label={groq ? "Groq requests used today" : "Free translation characters used this month"}
      >
        <div className="usage-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
