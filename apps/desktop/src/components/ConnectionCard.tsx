import { useState } from "react";
import type { Translator } from "../i18n";
import { localBaseUrl, type ProfileSummary } from "../model";
import { OverflowMenu } from "./OverflowMenu";

type Props = {
  onDelete: () => void;
  onRotate: () => void;
  profile: ProfileSummary;
  t: Translator;
};

const policyKey = {
  strict: "policyStrict",
  balanced: "policyBalanced",
  monitor: "policyMonitor",
} as const;

export function ConnectionCard({ onDelete, onRotate, profile, t }: Props) {
  const [copied, setCopied] = useState(false);
  const localUrl = localBaseUrl(profile.protocol);
  const copyUrl = async () => {
    await navigator.clipboard.writeText(localUrl);
    setCopied(true);
  };

  return (
    <article className="connection-card">
      <header>
        <div>
          <h3>{profile.name}</h3>
          <div className="badges">
            <span className="badge">
              {t(profile.protocol === "openai" ? "openaiCompatible" : "anthropicMessages")}
            </span>
            <span className="badge badge--policy">{t(policyKey[profile.policy])}</span>
          </div>
        </div>
        <OverflowMenu name={profile.name} onDelete={onDelete} onRotate={onRotate} t={t} />
      </header>
      <dl>
        <div>
          <dt>{t("upstream")}</dt>
          <dd>{profile.upstream_url}</dd>
        </div>
        <div>
          <dt>{t("localBaseUrl")}</dt>
          <dd><code>{localUrl}</code></dd>
        </div>
      </dl>
      <button className="copy-button" onClick={() => void copyUrl()} type="button">
        {copied ? t("copied") : t("copyBaseUrl")}
      </button>
    </article>
  );
}
