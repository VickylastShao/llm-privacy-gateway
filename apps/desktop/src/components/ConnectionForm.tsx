import { type FormEvent, useRef, useState } from "react";
import type { Translator } from "../i18n";
import {
  defaultUpstreamUrl,
  type Policy,
  type ProfileDraft,
  type Protocol,
} from "../model";

type Props = {
  t: Translator;
  onSubmit: (draft: ProfileDraft) => Promise<void>;
  onCancel?: () => void;
};

const protocols: Protocol[] = ["openai", "anthropic"];
const policies: Policy[] = ["strict", "balanced", "monitor"];
const policyCopy = {
  strict: ["policyStrict", "policyStrictDescription"],
  balanced: ["policyBalanced", "policyBalancedDescription"],
  monitor: ["policyMonitor", "policyMonitorDescription"],
} as const;

export function ConnectionForm({ t, onSubmit, onCancel }: Props) {
  const upstreamKey = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [protocol, setProtocol] = useState<Protocol>("openai");
  const [upstreamUrl, setUpstreamUrl] = useState(defaultUpstreamUrl("openai"));
  const [urlIsDirty, setUrlIsDirty] = useState(false);
  const [policy, setPolicy] = useState<Policy>("strict");
  const [pending, setPending] = useState(false);

  const chooseProtocol = (next: Protocol) => {
    setProtocol(next);
    if (!urlIsDirty) setUpstreamUrl(defaultUpstreamUrl(next));
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const key = upstreamKey.current?.value ?? "";
    setPending(true);
    try {
      await onSubmit({ name, protocol, upstreamUrl, upstreamKey: key, policy });
      setName("");
    } finally {
      if (upstreamKey.current) upstreamKey.current.value = "";
      setPending(false);
    }
  };

  return (
    <form aria-label={t("connectionForm")} onSubmit={(event) => void submit(event)}>
      <label>
        <span>{t("connectionName")}</span>
        <input
          autoFocus
          name="name"
          onChange={(event) => setName(event.target.value)}
          placeholder={t("connectionNamePlaceholder")}
          required
          value={name}
        />
      </label>

      <fieldset>
        <legend>{t("protocol")}</legend>
        <div className="segmented segmented--protocol">
          {protocols.map((value) => (
            <label key={value}>
              <input
                checked={protocol === value}
                name="protocol"
                onChange={() => chooseProtocol(value)}
                type="radio"
                value={value}
              />
              <span>{t(value === "openai" ? "openaiCompatible" : "anthropicMessages")}</span>
            </label>
          ))}
        </div>
      </fieldset>

      <div className="field">
        <label htmlFor="upstream-url">{t("upstreamBaseUrl")}</label>
        <div className="input-with-action">
          <input
            id="upstream-url"
            name="upstreamUrl"
            onChange={(event) => {
              setUpstreamUrl(event.target.value);
              setUrlIsDirty(true);
            }}
            required
            type="url"
            value={upstreamUrl}
          />
          <button
            aria-label={t("resetDefaultUrl")}
            onClick={() => {
              setUpstreamUrl(defaultUpstreamUrl(protocol));
              setUrlIsDirty(false);
            }}
            type="button"
          >
            ↺
          </button>
        </div>
      </div>

      <div className="field">
        <label htmlFor="upstream-key">{t("upstreamApiKey")}</label>
        <input
          aria-describedby="upstream-key-hint"
          autoComplete="off"
          id="upstream-key"
          name="upstreamKey"
          ref={upstreamKey}
          required
          type="password"
        />
        <small id="upstream-key-hint">{t("apiKeyHint")}</small>
      </div>

      <details>
        <summary>{t("advancedSettings")}</summary>
        <fieldset>
          <legend>{t("policy")}</legend>
          <div className="policy-options">
            {policies.map((value) => (
              <label key={value}>
                <input
                  checked={policy === value}
                  name="policy"
                  onChange={() => setPolicy(value)}
                  type="radio"
                  value={value}
                />
                <span>
                  <strong>{t(policyCopy[value][0])}</strong>
                  <small>{t(policyCopy[value][1])}</small>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
      </details>

      <div className="form-actions">
        {onCancel && (
          <button onClick={onCancel} type="button">
            {t("closeForm")}
          </button>
        )}
        <button disabled={pending} type="submit">
          {pending ? t("creatingConnection") : t("createConnection")}
        </button>
      </div>
    </form>
  );
}
