import { useEffect, useState } from "react";
import { ConnectionForm } from "./components/ConnectionForm";
import { OneTimeSecret } from "./components/OneTimeSecret";
import { desktopApi, type DesktopApi } from "./desktop-api";
import {
  initialLocale,
  localizeError,
  setLocalePreference,
  translate,
  type Locale,
  type TranslationKey,
} from "./i18n";
import type { CreatedProfile, GatewayStatus, ProfileDraft, ProfileSummary } from "./model";

type DisplayError = { summary: string; detail?: string };

export function App({
  api = desktopApi,
  initialLanguage,
}: {
  api?: DesktopApi;
  initialLanguage?: string;
}) {
  const [locale, setLocale] = useState<Locale>(() => initialLocale(initialLanguage));
  const [status, setStatus] = useState<GatewayStatus>({ running: false });
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [created, setCreated] = useState<CreatedProfile | null>(null);
  const [error, setError] = useState<DisplayError | null>(null);
  const [gatewayPending, setGatewayPending] = useState(false);
  const t = (key: TranslationKey, values?: Record<string, string>) =>
    translate(locale, key, values);

  const reportError = (cause: unknown) => setError(localizeError(locale, cause));

  useEffect(() => {
    void api.gatewayStatus().then(setStatus).catch(reportError);
    void api.listProfiles().then(setProfiles).catch(reportError);
  }, [api]);

  const changeLocale = (nextLocale: Locale) => {
    setLocale(nextLocale);
    setLocalePreference(nextLocale);
    setError(null);
  };

  const toggleGateway = async () => {
    setGatewayPending(true);
    try {
      setStatus(status.running ? await api.stopGateway() : await api.startGateway());
      setError(null);
    } catch (cause) {
      reportError(cause);
    } finally {
      setGatewayPending(false);
    }
  };

  const createProfile = async (draft: ProfileDraft) => {
    try {
      const next = await api.createProfile(draft);
      setProfiles((current) => [...current, next.profile]);
      setCreated(next);
      setError(null);
    } catch (cause) {
      reportError(cause);
      throw cause;
    }
  };

  const startLabel = gatewayPending
    ? status.running
      ? t("stoppingProtection")
      : t("startingProtection")
    : status.running
      ? t("stopProtection")
      : t("startProtection");

  return (
    <main className="app-shell" lang={locale}>
      <header className="app-header">
        <div className="brand">
          <h1>{t("appName")}</h1>
          <p>{t("appPurpose")}</p>
        </div>
        <div className="header-actions">
          <div aria-label={t("language")} className="segmented" role="group">
            <button
              aria-pressed={locale === "zh-CN"}
              onClick={() => changeLocale("zh-CN")}
              type="button"
            >
              {t("chinese")}
            </button>
            <button
              aria-pressed={locale === "en"}
              onClick={() => changeLocale("en")}
              type="button"
            >
              {t("english")}
            </button>
          </div>
          <div aria-label={t("statusRegion")} className="gateway-control" role="status">
            <span className={status.running ? "status status--running" : "status"}>
              {status.running ? t("gatewayRunning") : t("gatewayStopped")}
            </span>
            <button
              disabled={profiles.length === 0 || gatewayPending}
              onClick={() => void toggleGateway()}
              type="button"
            >
              {startLabel}
            </button>
          </div>
        </div>
      </header>

      <div className="workflow">
        {profiles.length === 0 && (
          <section aria-labelledby="first-run-title" className="intro-card">
            <h2 id="first-run-title">{t("firstRunTitle")}</h2>
            <p>{t("firstRunDescription")}</p>
            <ol className="steps">
              <li>{t("stepAdd")}</li>
              <li>{t("stepCopy")}</li>
              <li>{t("stepStart")}</li>
            </ol>
          </section>
        )}

        {created && (
          <OneTimeSecret created={created} onDismiss={() => setCreated(null)} t={t} />
        )}

        <section aria-labelledby="connections-title" className="connections-section">
          <div className="section-heading">
            <div>
              <h2 id="connections-title">{t("connectionsTitle")}</h2>
              <p>{t("connectionsDescription")}</p>
            </div>
          </div>
          <ConnectionForm onSubmit={createProfile} t={t} />
          {profiles.length > 0 && (
            <ul aria-label={t("connectionList")}>
              {profiles.map((profile) => (
                <li key={profile.id}>{profile.name}</li>
              ))}
            </ul>
          )}
        </section>

        {profiles.length === 0 && <p className="helper">{t("needConnection")}</p>}
        {error && (
          <div className="error-notice" role="alert">
            <strong>{error.summary}</strong>
            {error.detail && (
              <details>
                <summary>{t("technicalDetail")}</summary>
                <code>{error.detail}</code>
              </details>
            )}
          </div>
        )}
      </div>
    </main>
  );
}
