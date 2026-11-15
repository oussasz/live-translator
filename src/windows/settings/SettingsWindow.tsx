import { useEffect, useState } from "react";
import { cmd } from "../../lib/ipc";
import type { KeyStatus, PlatformStatus, Settings, Theme, UsageSnapshot } from "../../lib/types";
import { providerInfo } from "../../lib/providers";
import { ServicePicker } from "./ServicePicker";
import { FirstRun } from "./FirstRun";
import { HotkeyRecorder } from "./HotkeyRecorder";
import { KeyForm } from "./KeyForm";
import { LanguagePicker } from "./LanguagePicker";
import { UsageBar } from "./UsageBar";
import "../popup.css";
import "./settings.css";

const SHORTCUTS: { key: keyof Settings["hotkeys"]; label: string }[] = [
  { key: "toggle", label: "Translate mode" },
  { key: "cycle_target", label: "Cycle language" },
  { key: "read_selection", label: "Translate selection" },
];

function Switch({ id, checked, onChange }: { id: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <span className="switch">
      <input id={id} type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </span>
  );
}

export default function SettingsWindow() {
  const [keys, setKeys] = useState<KeyStatus | null>(null);
  const [onboarding, setOnboarding] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [usage, setUsage] = useState<UsageSnapshot | null>(null);
  const [changingKey, setChangingKey] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [platform, setPlatform] = useState<PlatformStatus | null>(null);

  useEffect(() => {
    Promise.all([cmd.hasApiKey(), cmd.getSettings()]).then(([k, st]) => {
      setKeys(k);
      setSettings(st);
      setOnboarding(!k[st.provider]);
    });
    cmd.platformStatus?.().then(setPlatform).catch(() => {});
    const loadUsage = () => cmd.getUsage().then(setUsage).catch(() => {});
    loadUsage();
    const t = setInterval(loadUsage, 10000);
    return () => clearInterval(t);
  }, []);

  if (keys === null || !settings) return null;

  if (onboarding) {
    return (
      <div className="settings">
        <FirstRun
          settings={settings}
          onDone={(s) => {
            setSettings(s);
            setKeys({ ...keys, [s.provider]: true });
            setOnboarding(false);
          }}
        />
      </div>
    );
  }

  const update = async (next: Settings) => {
    setError(null);
    try {
      await cmd.saveSettings(next);
      setSettings(next);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="settings">
      <div className="settings-inner">
        <div>
          <h1>Settings</h1>
          <p className="subtitle">
            Press {settings.hotkeys.toggle} in any text field to type in your language and send the translation.
          </p>
        </div>

        {error && <div className="banner-error">{error}</div>}
        {platform?.problem && (
          <div className="banner-error" role="alert">
            <strong>{platform.problem}</strong>
            {platform.fix && <div style={{ marginTop: 4, userSelect: "text" }}>{platform.fix}</div>}
          </div>
        )}

        <section className="card" aria-labelledby="h-key">
          <h2 id="h-key">Translation service</h2>
          <ServicePicker
            provider={settings.provider}
            model={settings.groq_model}
            onProvider={(p) => {
              setChangingKey(false);
              update({ ...settings, provider: p });
            }}
            onModel={(m) => update({ ...settings, groq_model: m })}
          />
          {changingKey || !keys[settings.provider] ? (
            <KeyForm
              key={settings.provider}
              provider={settings.provider}
              onSaved={() => {
                setKeys({ ...keys, [settings.provider]: true });
                setChangingKey(false);
              }}
            />
          ) : (
            <div className="row">
              <span className="status-ok">✓ {providerInfo(settings.provider).name} connected</span>
              <button className="btn" onClick={() => setChangingKey(true)}>
                Change key
              </button>
            </div>
          )}
        </section>

        <section className="card" aria-labelledby="h-langs">
          <h2 id="h-langs">Languages</h2>
          <LanguagePicker
            value={{ favorites: settings.favorites, target: settings.target }}
            onChange={(v) => update({ ...settings, ...v })}
          />
        </section>

        <section className="card" aria-labelledby="h-keys">
          <h2 id="h-keys">Shortcuts</h2>
          {SHORTCUTS.map((s) => (
            <HotkeyRecorder
              key={s.key}
              label={s.label}
              value={settings.hotkeys[s.key]}
              others={SHORTCUTS.filter((o) => o.key !== s.key).map((o) => ({ label: o.label, value: settings.hotkeys[o.key] }))}
              onChange={(combo) => update({ ...settings, hotkeys: { ...settings.hotkeys, [s.key]: combo } })}
            />
          ))}
          <p className="hint">Esc leaves translate mode. Enter sends your message once the translation is ready.</p>
        </section>

        <section className="card" aria-labelledby="h-look">
          <h2 id="h-look">Appearance</h2>
          <div className="row">
            <span className="row-label">Theme</span>
            <div className="radio-group" role="radiogroup" aria-label="Theme">
              {(["system", "light", "dark"] as Theme[]).map((t) => (
                <label key={t}>
                  <input
                    type="radio"
                    name="theme"
                    value={t}
                    checked={settings.theme === t}
                    onChange={() => update({ ...settings, theme: t })}
                  />
                  {t[0].toUpperCase() + t.slice(1)}
                </label>
              ))}
            </div>
          </div>
        </section>

        <section className="card" aria-labelledby="h-start">
          <h2 id="h-start">Startup</h2>
          <div className="row">
            <label className="row-label" htmlFor="autostart">
              Start with Windows
            </label>
            <Switch id="autostart" checked={settings.autostart} onChange={(v) => update({ ...settings, autostart: v })} />
          </div>
        </section>

        <section className="card" aria-labelledby="h-usage">
          <h2 id="h-usage">Usage</h2>
          {usage && <UsageBar usage={usage} provider={settings.provider} />}
          {settings.provider === "google" ? (
            <>
              <div className="row">
                <label className="row-label" htmlFor="cap">
                  Stop at the free limit
                </label>
                <Switch id="cap" checked={settings.cap_enabled} onChange={(v) => update({ ...settings, cap_enabled: v })} />
              </div>
              <p className="hint">Google gives 500,000 characters free each month. With this on, you never pay.</p>
            </>
          ) : (
            <p className="hint">
              Groq’s free plan allows 1,000 requests a day and 30 a minute. The app waits a little longer before
              translating and paces itself to stay inside it.
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
