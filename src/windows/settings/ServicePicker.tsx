import { GROQ_MODELS, PROVIDERS } from "../../lib/providers";
import type { Provider } from "../../lib/types";

interface Props {
  provider: Provider;
  model: string;
  onProvider: (p: Provider) => void;
  onModel: (m: string) => void;
}

export function ServicePicker({ provider, model, onProvider, onModel }: Props) {
  return (
    <div className="field">
      <div className="row">
        <span className="row-label">Service</span>
        <div className="radio-group" role="radiogroup" aria-label="Translation service">
          {PROVIDERS.map((p) => (
            <label key={p.id}>
              <input type="radio" name="provider" value={p.id} checked={provider === p.id} onChange={() => onProvider(p.id)} />
              {p.name}
            </label>
          ))}
        </div>
      </div>
      {provider === "groq" && (
        <div className="row">
          <label className="row-label" htmlFor="groq-model">
            Groq model
          </label>
          <select id="groq-model" className="select" value={model} onChange={(e) => onModel(e.target.value)}>
            {GROQ_MODELS.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  );
}
