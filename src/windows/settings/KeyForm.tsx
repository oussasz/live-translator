import { useState } from "react";
import { cmd } from "../../lib/ipc";
import { keyErrorCopy } from "./keyErrors";

export function KeyForm({ onSaved }: { onSaved: () => void }) {
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
      await cmd.setApiKey(key.trim());
      onSaved();
    } catch (e) {
      setError(keyErrorCopy(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="field"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <label htmlFor="api-key">Google API key</label>
      <div className="input-wrap">
        <input
          id="api-key"
          className="input"
          type={show ? "text" : "password"}
          value={key}
          autoComplete="off"
          spellCheck={false}
          placeholder="AIzaSy…"
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
      <p className="hint">
        Create a key in Google Cloud Console under APIs and Services, Credentials, with the Cloud Translation API
        enabled. The key is stored in Windows Credential Manager on this PC only.
      </p>
    </form>
  );
}
