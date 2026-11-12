import type { UsageSnapshot } from "../../lib/types";

export function UsageBar({ usage }: { usage: UsageSnapshot }) {
  const pct = Math.min(100, Math.round((usage.chars / usage.limit) * 100));
  const fmt = (n: number) => n.toLocaleString("en-US");
  return (
    <div className="field">
      <div className="row">
        <span className="row-label">
          This month: {fmt(usage.chars)} / {fmt(usage.limit)} free
        </span>
        <span className="hint">{pct}%</span>
      </div>
      <div
        className="usage-track"
        data-testid="usage-bar"
        data-warn={pct >= 80 ? "true" : "false"}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={usage.limit}
        aria-valuenow={usage.chars}
        aria-label="Free translation characters used this month"
      >
        <div className="usage-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
