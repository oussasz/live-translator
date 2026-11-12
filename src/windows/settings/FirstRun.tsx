import { useState } from "react";
import { cmd } from "../../lib/ipc";
import type { Settings } from "../../lib/types";
import { KeyForm } from "./KeyForm";
import { LanguagePicker } from "./LanguagePicker";

export function FirstRun({ settings, onDone }: { settings: Settings; onDone: (s: Settings) => void }) {
  const [step, setStep] = useState<"key" | "langs">("key");
  const [langs, setLangs] = useState({ favorites: settings.favorites, target: settings.target });
  const [error, setError] = useState<string | null>(null);

  const finish = async () => {
    const next = { ...settings, ...langs };
    try {
      await cmd.saveSettings(next);
      onDone(next);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="settings-inner">
      <span className="steps">Step {step === "key" ? 1 : 2} of 2</span>
      {step === "key" ? (
        <>
          <div>
            <h1>Connect Google Translate</h1>
            <p className="subtitle">Paste your Google Cloud API key. It’s tested once, then saved securely.</p>
          </div>
          <div className="card">
            <KeyForm onSaved={() => setStep("langs")} />
          </div>
        </>
      ) : (
        <>
          <div>
            <h1>Pick your languages</h1>
            <p className="subtitle">Choose the languages you write in and the one to translate into.</p>
          </div>
          <div className="card">
            <LanguagePicker value={langs} onChange={setLangs} />
          </div>
          {error && <div className="banner-error">{error}</div>}
          <div className="actions">
            <button className="btn primary" onClick={finish}>
              Done
            </button>
          </div>
        </>
      )}
    </div>
  );
}
