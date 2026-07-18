import { useEffect, useRef, useState } from "react";
import type { Translator } from "../i18n";
import type { CreatedProfile } from "../model";

type Props = { created: CreatedProfile; onDismiss: () => void; t: Translator };

export function OneTimeSecret({ created, onDismiss, t }: Props) {
  const container = useRef<HTMLElement>(null);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => container.current?.focus(), [created.local_key]);

  const copy = async (name: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(name);
  };
  const isAnthropic = created.profile.protocol === "anthropic";
  const baseVariable = isAnthropic ? "ANTHROPIC_BASE_URL" : "OPENAI_BASE_URL";
  const keyVariable = isAnthropic ? "ANTHROPIC_API_KEY" : "OPENAI_API_KEY";
  const full = `${baseVariable}=${created.local_base_url}\n${keyVariable}=${created.local_key}`;

  return (
    <article
      aria-label={t("oneTimeTitle")}
      className="one-time-secret"
      ref={container}
      role="region"
      tabIndex={-1}
    >
      <div>
        <h2>{t("oneTimeTitle")}</h2>
        <p>{t("oneTimeDescription")}</p>
      </div>
      <dl>
        <div>
          <dt>{t("localBaseUrl")}</dt>
          <dd><code>{created.local_base_url}</code></dd>
        </div>
        <div>
          <dt>{t("localApiKey")}</dt>
          <dd><code>{created.local_key}</code></dd>
        </div>
      </dl>
      <div className="secret-actions">
        <button onClick={() => void copy("url", created.local_base_url)} type="button">
          {copied === "url" ? t("copied") : t("copyBaseUrl")}
        </button>
        <button onClick={() => void copy("key", created.local_key)} type="button">
          {copied === "key" ? t("copied") : t("copyKey")}
        </button>
        <button onClick={() => void copy("full", full)} type="button">
          {copied === "full" ? t("copied") : t("copyFullConfiguration")}
        </button>
        <button onClick={onDismiss} type="button">{t("dismissSecret")}</button>
      </div>
    </article>
  );
}
