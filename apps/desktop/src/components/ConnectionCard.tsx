import { useState } from "react";
import type { Translator } from "../i18n";
import {
  localBaseUrl,
  supportsBalanceQuery,
  type BalanceSummary,
  type ConnectionTestSummary,
  type ProfileSummary,
  type UsageTotals,
} from "../model";
import { OverflowMenu } from "./OverflowMenu";

type Props = {
  onDelete: () => void;
  onEdit: () => void;
  onRotate: () => void;
  onQueryBalance: () => Promise<BalanceSummary>;
  onTestConnection: () => Promise<ConnectionTestSummary>;
  profile: ProfileSummary;
  usage?: UsageTotals;
  t: Translator;
};

const policyKey = {
  strict: "policyStrict",
  balanced: "policyBalanced",
  monitor: "policyMonitor",
} as const;

export function ConnectionCard({
  onDelete,
  onEdit,
  onRotate,
  onQueryBalance,
  onTestConnection,
  profile,
  usage,
  t,
}: Props) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const [balance, setBalance] = useState<BalanceSummary | null>(null);
  const [balancePending, setBalancePending] = useState(false);
  const [balanceError, setBalanceError] = useState(false);
  const [connectionTest, setConnectionTest] = useState<ConnectionTestSummary | null>(null);
  const [connectionTestPending, setConnectionTestPending] = useState(false);
  const [connectionTestError, setConnectionTestError] = useState(false);
  const localUrl = localBaseUrl(profile.protocol);
  const balanceSupported = supportsBalanceQuery(profile.upstream_url);
  const copyUrl = async () => {
    setCopyError(false);
    try {
      await navigator.clipboard.writeText(localUrl);
      setCopied(true);
    } catch {
      setCopied(false);
      setCopyError(true);
    }
  };
  const refreshBalance = async () => {
    setBalancePending(true);
    setBalanceError(false);
    try {
      setBalance(await onQueryBalance());
    } catch {
      setBalanceError(true);
    } finally {
      setBalancePending(false);
    }
  };
  const testConnection = async () => {
    setConnectionTestPending(true);
    setConnectionTestError(false);
    try {
      setConnectionTest(await onTestConnection());
    } catch {
      setConnectionTest(null);
      setConnectionTestError(true);
    } finally {
      setConnectionTestPending(false);
    }
  };
  const number = (value: number) => value.toLocaleString();

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
        <OverflowMenu
          name={profile.name}
          onDelete={onDelete}
          onEdit={onEdit}
          onRotate={onRotate}
          t={t}
        />
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
      {profile.custom_term_count > 0 && (
        <small className="custom-term-count">
          {t("customTermsConfigured", { count: number(profile.custom_term_count) })}
        </small>
      )}
      <div className="usage-summary" aria-label={t("usageSummary")}>
        <div className="usage-summary__heading">
          <strong>{t("usageSummary")}</strong>
          <small>{t("usageRetention")}</small>
        </div>
        {usage ? (
          <>
            <dl className="usage-metrics">
              <div><dt>{t("usageRequests")}</dt><dd>{number(usage.request_count)}</dd></div>
              <div><dt>{t("usageTotal")}</dt><dd>{number(usage.total_tokens)}</dd></div>
              <div><dt>{t("usageInput")}</dt><dd>{number(usage.input_tokens)}</dd></div>
              <div><dt>{t("usageOutput")}</dt><dd>{number(usage.output_tokens)}</dd></div>
              <div><dt>{t("usageCached")}</dt><dd>{number(usage.cached_tokens)}</dd></div>
            </dl>
            <small>
              {t("usageCoverage", {
                metered: number(usage.metered_request_count),
                requests: number(usage.request_count),
              })}
            </small>
          </>
        ) : (
          <small>{t("usageEmpty")}</small>
        )}
        {balance?.supported && (
          <p className="balance-result">
            {balance.remaining !== undefined
              ? t(
                  balance.metric === "api_key_limit"
                    ? "balanceKeyRemaining"
                    : "balanceAccountRemaining",
                  {
                    value: balance.remaining.toLocaleString(undefined, {
                      maximumFractionDigits: 4,
                    }),
                    unit: balance.unit ?? "",
                  },
                )
              : balance.used !== undefined
                ? t("balanceUsed", {
                    value: balance.used.toLocaleString(undefined, { maximumFractionDigits: 4 }),
                    unit: balance.unit ?? "",
                  })
                : t("balanceUnavailable")}
          </p>
        )}
        {!balanceSupported && <small>{t("balanceUnsupported")}</small>}
        {balanceError && <small className="balance-error">{t("balanceQueryFailed")}</small>}
        {balanceSupported && (
          <button disabled={balancePending} onClick={() => void refreshBalance()} type="button">
            {balancePending ? t("balanceQuerying") : t("balanceQuery")}
          </button>
        )}
      </div>
      {connectionTest && (
        <small className={connectionTest.success ? "connection-ok" : "connection-error"}>
          {connectionTest.success
            ? t("connectionTestSucceeded", {
                latency: number(connectionTest.latency_ms),
              })
            : t("connectionTestRejected", { status: String(connectionTest.status) })}
        </small>
      )}
      {connectionTestError && <small className="connection-error">{t("connectionTestFailed")}</small>}
      <div className="connection-actions">
        <button className="copy-button" onClick={() => void copyUrl()} type="button">
          {copied ? t("copied") : t("copyBaseUrl")}
        </button>
        <button disabled={connectionTestPending} onClick={() => void testConnection()} type="button">
          {connectionTestPending ? t("connectionTesting") : t("connectionTest")}
        </button>
      </div>
      {copyError && <small className="copy-error" role="alert">{t("copyFailed")}</small>}
    </article>
  );
}
