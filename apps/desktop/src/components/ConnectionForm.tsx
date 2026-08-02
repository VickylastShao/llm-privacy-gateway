import { type FormEvent, useRef, useState } from "react";
import type { Translator } from "../i18n";
import {
  providerDefaultUrl,
  providerPreset,
  providerPresets,
  providerProtocols,
  sameUpstreamOrigin,
  type DiscoveredProfile,
  type EditableProfile,
  type Policy,
  type ProfileDraft,
  type ProviderId,
  type Protocol,
} from "../model";

type Props = {
  t: Translator;
  onSubmit: (draft: ProfileDraft) => Promise<void>;
  discoveries: DiscoveredProfile[];
  discoveryScanned: boolean;
  discoveryPending: boolean;
  onScanEnvironment: () => Promise<void>;
  onCancel?: () => void;
  initial?: EditableProfile;
};

const protocols: Protocol[] = ["openai", "anthropic"];
const policies: Policy[] = ["strict", "balanced", "monitor"];
const policyCopy = {
  strict: ["policyStrict", "policyStrictDescription"],
  balanced: ["policyBalanced", "policyBalancedDescription"],
  monitor: ["policyMonitor", "policyMonitorDescription"],
} as const;

export function ConnectionForm({
  t,
  onSubmit,
  onCancel,
  discoveries,
  discoveryScanned,
  discoveryPending,
  onScanEnvironment,
  initial,
}: Props) {
  const upstreamKey = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(initial?.name ?? "");
  const [provider, setProvider] = useState<ProviderId>(initial?.provider ?? "openai");
  const [protocol, setProtocol] = useState<Protocol>(initial?.protocol ?? "openai");
  const [upstreamUrl, setUpstreamUrl] = useState(
    initial?.upstream_url ?? providerDefaultUrl("openai", "openai"),
  );
  const [urlIsDirty, setUrlIsDirty] = useState(Boolean(initial));
  const [discoveryId, setDiscoveryId] = useState<string | undefined>();
  const [policy, setPolicy] = useState<Policy>(initial?.policy ?? "strict");
  const [customTerms, setCustomTerms] = useState(initial?.custom_terms.join("\n") ?? "");
  const [pending, setPending] = useState(false);
  const upstreamOriginChanged = Boolean(
    initial && !sameUpstreamOrigin(initial.upstream_url, upstreamUrl),
  );

  const chooseProtocol = (next: Protocol) => {
    setProtocol(next);
    if (!urlIsDirty) setUpstreamUrl(providerDefaultUrl(provider, next));
  };

  const chooseProvider = (next: ProviderId) => {
    const preset = providerPreset(next);
    const supported = providerProtocols(next);
    const nextProtocol = supported.includes(protocol) ? protocol : (supported[0] ?? protocol);
    setProvider(next);
    setProtocol(nextProtocol);
    setUpstreamUrl(providerDefaultUrl(next, nextProtocol));
    setUrlIsDirty(false);
    setDiscoveryId(undefined);
    if (!name.trim() && next !== "custom") setName(preset.name);
  };

  const chooseDiscovery = (candidate: DiscoveredProfile) => {
    setDiscoveryId(candidate.id);
    setProvider(candidate.provider);
    setProtocol(candidate.protocol);
    setName(candidate.name);
    setUpstreamUrl(candidate.upstream_url);
    setUrlIsDirty(false);
    if (upstreamKey.current) upstreamKey.current.value = "";
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const key = upstreamKey.current?.value ?? "";
    setPending(true);
    try {
      await onSubmit({
        name,
        protocol,
        upstreamUrl,
        upstreamKey: key,
        discoveryId,
        policy,
        customTerms: customTerms
          .split(/\r?\n/)
          .map((term) => term.trim())
          .filter(Boolean),
      });
      setName("");
      setCustomTerms("");
      setDiscoveryId(undefined);
    } finally {
      if (upstreamKey.current) upstreamKey.current.value = "";
      setPending(false);
    }
  };

  return (
    <form aria-label={t("connectionForm")} onSubmit={(event) => void submit(event)}>
      {!initial && <section className="environment-import" aria-label={t("environmentImportTitle")}>
        <div>
          <strong>{t("environmentImportTitle")}</strong>
          <small>{t("environmentImportDescription")}</small>
        </div>
        <button
          disabled={discoveryPending}
          onClick={() => void onScanEnvironment()}
          type="button"
        >
          {discoveryPending ? t("environmentScanning") : t("environmentScan")}
        </button>
        {discoveries.length > 0 && (
          <div className="discovery-list">
            {discoveries.map((candidate) => (
              <button
                aria-pressed={discoveryId === candidate.id}
                className="discovery-candidate"
                key={candidate.id}
                onClick={() => chooseDiscovery(candidate)}
                type="button"
              >
                <span>
                  <strong>{candidate.name}</strong>
                  <small>
                    {candidate.source} · {candidate.key_variable}
                    {candidate.url_variable ? ` + ${candidate.url_variable}` : ""}
                  </small>
                </span>
                <code>{candidate.key_preview}</code>
              </button>
            ))}
          </div>
        )}
        {discoveryScanned && !discoveryPending && discoveries.length === 0 && (
          <small>{t("environmentEmpty")}</small>
        )}
      </section>}

      <div className="field">
        <label htmlFor="provider">{t("provider")}</label>
        <div className="select-shell">
          <select
            id="provider"
            onChange={(event) => chooseProvider(event.target.value as ProviderId)}
            value={provider}
          >
            {providerPresets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.id === "custom" ? t("providerCustom") : preset.name}
              </option>
            ))}
          </select>
        </div>
        <small>{t("providerHint")}</small>
      </div>

      <label>
        <span>{t("connectionName")}</span>
        <input
          name="name"
          onChange={(event) => setName(event.target.value)}
          placeholder={t("connectionNamePlaceholder")}
          required
          value={name}
        />
      </label>

      {providerProtocols(provider).length > 1 && <fieldset>
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
      </fieldset>}

      <div className="field">
        <label htmlFor="upstream-url">{t("upstreamBaseUrl")}</label>
        <div className="input-with-action">
          <input
            id="upstream-url"
            name="upstreamUrl"
            onChange={(event) => {
              setUpstreamUrl(event.target.value);
              setUrlIsDirty(true);
              setDiscoveryId(undefined);
            }}
            required
            type="url"
            value={upstreamUrl}
          />
          <button
            aria-label={t("resetDefaultUrl")}
            onClick={() => {
              setUpstreamUrl(providerDefaultUrl(provider, protocol));
              setUrlIsDirty(false);
              setDiscoveryId(undefined);
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
          required={!discoveryId && (!initial || upstreamOriginChanged)}
          type="password"
        />
        <small id="upstream-key-hint">
          {initial
            ? t(upstreamOriginChanged ? "apiKeyOriginChangedHint" : "apiKeyEditHint")
            : discoveryId
              ? t("apiKeyImportedHint")
              : t("apiKeyHint")}
        </small>
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
        <div className="field custom-terms-field">
          <label htmlFor="custom-terms">{t("customTerms")}</label>
          <textarea
            aria-describedby="custom-terms-hint"
            id="custom-terms"
            onChange={(event) => setCustomTerms(event.target.value)}
            placeholder={t("customTermsPlaceholder")}
            rows={4}
            value={customTerms}
          />
          <small id="custom-terms-hint">{t("customTermsHint")}</small>
        </div>
      </details>

      <div className="form-actions">
        {onCancel && (
          <button onClick={onCancel} type="button">
            {t("closeForm")}
          </button>
        )}
        <button disabled={pending} type="submit">
          {pending
            ? t(initial ? "updatingConnection" : "creatingConnection")
            : t(initial ? "updateConnection" : "createConnection")}
        </button>
      </div>
    </form>
  );
}
