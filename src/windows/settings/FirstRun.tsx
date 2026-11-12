import { useState } from "react";
import { cmd } from "../../lib/ipc";
import type { Settings } from "../../lib/types";
import { KeyForm } from "./KeyForm";
import { LanguagePicker } from "./LanguagePicker";
import { ServicePicker } from "./ServicePicker";

export function FirstRun({ settings, onDone }: { settings: Settings; onDone: (s: Settings) => void }) {
  const [step, setStep] = useState<"key" | "langs">("key");
  const [provider, setProvider] = useState(settings.provider);
  const [model, setModel] = useState(settings.groq_model);
  const [langs, setLangs] = useState({ favorites: settings.favorites, target: settings.target });
  const [error, setError] = useState<string | null>(null);

  const finish = async () => {
    const next = { ...settings, ...langs, provider, groq_model: model };
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
            <h1>Connect your translation service</h1>
            <p className="subtitle">Pick a service and paste its API key. It’s tested once, then saved securely.</p>
          </div>
          <div className="card">
            <ServicePicker provider={provider} model={model} onProvider={setProvider} onModel={setModel} />
            <KeyForm key={provider} provider={provider} onSaved={() => setStep("langs")} />
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
