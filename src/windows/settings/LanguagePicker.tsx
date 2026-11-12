import { LANG_NAMES } from "../../lib/langs";

const ALL = Object.keys(LANG_NAMES);

export interface LangValue {
  favorites: string[];
  target: string;
}

export function LanguagePicker({ value, onChange }: { value: LangValue; onChange: (v: LangValue) => void }) {
  const ordered = [...value.favorites, ...ALL.filter((c) => !value.favorites.includes(c))];

  const toggle = (code: string, on: boolean) => {
    const favorites = on ? [...value.favorites, code] : value.favorites.filter((c) => c !== code);
    const target = favorites.includes(value.target) ? value.target : favorites[0] ?? value.target;
    onChange({ favorites, target });
  };
  const move = (code: string, dir: -1 | 1) => {
    const f = [...value.favorites];
    const i = f.indexOf(code);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= f.length) return;
    [f[i], f[j]] = [f[j], f[i]];
    onChange({ ...value, favorites: f });
  };

  return (
    <div className="field">
      {ordered.map((code) => {
        const fav = value.favorites.includes(code);
        return (
          <div className="lang-row" key={code}>
            <input
              type="checkbox"
              id={`lang-${code}`}
              checked={fav}
              onChange={(e) => toggle(code, e.target.checked)}
            />
            <label className="name" htmlFor={`lang-${code}`}>
              {LANG_NAMES[code]}
            </label>
            {fav && (
              <>
                <button type="button" className="mini-btn" aria-label={`Move ${LANG_NAMES[code]} up`} onClick={() => move(code, -1)}>
                  ↑
                </button>
                <button type="button" className="mini-btn" aria-label={`Move ${LANG_NAMES[code]} down`} onClick={() => move(code, 1)}>
                  ↓
                </button>
              </>
            )}
          </div>
        );
      })}
      <div className="row">
        <label className="row-label" htmlFor="target">
          Translate into
        </label>
        <select
          id="target"
          className="select"
          value={value.target}
          onChange={(e) => onChange({ ...value, target: e.target.value })}
        >
          {value.favorites.map((c) => (
            <option key={c} value={c}>
              {LANG_NAMES[c]}
            </option>
          ))}
        </select>
      </div>
      <p className="hint">The order sets how the language shortcut cycles. The source language is detected automatically.</p>
    </div>
  );
}
