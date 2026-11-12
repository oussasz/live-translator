import { useState } from "react";
import { cmd } from "../../lib/ipc";
import { providerInfo } from "../../lib/providers";
import type { Provider } from "../../lib/types";
import { keyErrorCopy } from "./keyErrors";

export function KeyForm({ provider, onSaved }: { provider: Provider; onSaved: () => void }) {
  const info = providerInfo(provider);
  const [key, setKey] = useState("");
  const [show, setShow] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!key.trim()) {
      setError("Paste your API key first.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await cmd.setApiKey(provider, key.trim());
      setKey("");
      onSaved();
    } catch (e) {
      setError(keyErrorCopy(e));
    } finally {
      setBusy(false);
    }
  };

  const id = `api-key-${provider}`;
  return (
    <form
      className="field"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <label htmlFor={id}>{info.keyLabel}</label>
      <div className="input-wrap">
        <input
          id={id}
          className="input"
          type={show ? "text" : "password"}
          value={key}
          autoComplete="off"
          spellCheck={false}
          placeholder={info.placeholder}
          onChange={(e) => {
            setKey(e.target.value);
            setError(null);
          }}
        />
        <button type="button" className="btn" onClick={() => setShow(!show)}>
          {show ? "Hide" : "Show"}
        </button>
        <button type="submit" className="btn primary" disabled={busy}>
          {busy ? "Testing…" : "Test and save"}
        </button>
      </div>
      {error && <p className="error-text">{error}</p>}
      <p className="hint">{info.help} The key is stored in Windows Credential Manager on this PC only.</p>
    </form>
  );
}
