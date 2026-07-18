import { useEffect, useState } from "react";
import { ConfirmDialog, type ProfileAction } from "./components/ConfirmDialog";
import { ConnectionCard } from "./components/ConnectionCard";
import { ConnectionForm } from "./components/ConnectionForm";
import { OneTimeSecret } from "./components/OneTimeSecret";
import { PrivacyActivity } from "./components/PrivacyActivity";
import { desktopApi, type DesktopApi } from "./desktop-api";
import {
  initialLocale,
  localizeError,
  setLocalePreference,
  translate,
  type Locale,
  type TranslationKey,
} from "./i18n";
import {
  localBaseUrl,
  type CreatedProfile,
  type GatewayStatus,
  type ProfileDraft,
  type PrivacyActivityResponse,
  type ProfileSummary,
} from "./model";

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
  const [profilesLoaded, setProfilesLoaded] = useState(false);
  const [showForm, setShowForm] = useState(true);
  const [created, setCreated] = useState<CreatedProfile | null>(null);
  const [error, setError] = useState<DisplayError | null>(null);
  const [gatewayPending, setGatewayPending] = useState(false);
  const [profileAction, setProfileAction] = useState<{
    action: ProfileAction;
    profile: ProfileSummary;
  } | null>(null);
  const [profileActionPending, setProfileActionPending] = useState(false);
  const [activity, setActivity] = useState<PrivacyActivityResponse | null>(null);
  const [activityError, setActivityError] = useState(false);
  const [activityLimit, setActivityLimit] = useState(10);
  const t = (key: TranslationKey, values?: Record<string, string>) =>
    translate(locale, key, values);

  const reportError = (cause: unknown) => setError(localizeError(locale, cause));

  useEffect(() => {
    void api.gatewayStatus().then(setStatus).catch(reportError);
    void api
      .listProfiles()
      .then((next) => {
        setProfiles(next);
        setShowForm(next.length === 0);
      })
      .catch(reportError)
      .finally(() => setProfilesLoaded(true));
  }, [api]);

  useEffect(() => {
    let active = true;
    void api
      .queryActivity(activityLimit)
      .then((next) => {
        if (active) {
          setActivity(next);
          setActivityError(false);
        }
      })
      .catch(() => {
        if (active) setActivityError(true);
      });
    return () => {
      active = false;
    };
  }, [api, activityLimit]);

  useEffect(() => {
    if (!status.running) return;
    const timer = window.setInterval(() => {
      void api
        .queryActivity(activityLimit)
        .then((next) => {
          setActivity(next);
          setActivityError(false);
        })
        .catch(() => setActivityError(true));
    }, 1_000);
    return () => window.clearInterval(timer);
  }, [api, activityLimit, status.running]);

  const changeLocale = (nextLocale: Locale) => {
    setLocale(nextLocale);
    setLocalePreference(nextLocale);
    setError(null);
  };

  const toggleGateway = async () => {
    setGatewayPending(true);
    try {
      const nextStatus = status.running ? await api.stopGateway() : await api.startGateway();
      setStatus(nextStatus);
      if (!nextStatus.running) {
        try {
          const nextActivity = await api.queryActivity(activityLimit);
          setActivity(nextActivity);
          setActivityError(false);
        } catch {
          setActivityError(true);
        }
      }
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
      setShowForm(false);
      setError(null);
    } catch (cause) {
      reportError(cause);
      throw cause;
    }
  };

  const confirmProfileAction = async () => {
    if (!profileAction || status.running) return;
    setProfileActionPending(true);
    try {
      if (profileAction.action === "rotate") {
        const rotated = await api.rotateLocalKey(profileAction.profile.id);
        setCreated({
          profile: profileAction.profile,
          local_base_url: localBaseUrl(profileAction.profile.protocol),
          local_key: rotated.local_key,
        });
      } else {
        await api.deleteProfile(profileAction.profile.id);
        setProfiles((current) =>
          current.filter((profile) => profile.id !== profileAction.profile.id),
        );
        setShowForm(profiles.length === 1);
      }
      setError(null);
      setProfileAction(null);
    } catch (cause) {
      reportError(cause);
    } finally {
      setProfileActionPending(false);
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
            {profiles.length > 0 && !showForm && (
              <button onClick={() => setShowForm(true)} type="button">
                {t("addConnection")}
              </button>
            )}
          </div>
          {profilesLoaded && showForm && (
            <ConnectionForm
              onCancel={profiles.length > 0 ? () => setShowForm(false) : undefined}
              onSubmit={createProfile}
              t={t}
            />
          )}
          {profiles.length > 0 && (
            <div aria-label={t("connectionList")} className="connection-grid" role="list">
              {profiles.map((profile) => (
                <div key={profile.id} role="listitem">
                  <ConnectionCard
                    onDelete={() => setProfileAction({ action: "delete", profile })}
                    onRotate={() => setProfileAction({ action: "rotate", profile })}
                    profile={profile}
                    t={t}
                  />
                </div>
              ))}
            </div>
          )}
        </section>

        {profiles.length > 0 && (
          <PrivacyActivity
            data={activity}
            error={activityError ? t("activityQueryFailed") : null}
            limit={activityLimit}
            locale={locale}
            onShowMore={() => setActivityLimit(100)}
            profiles={profiles}
            t={t}
          />
        )}

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
      {profileAction && (
        <ConfirmDialog
          action={profileAction.action}
          blocked={status.running}
          onCancel={() => setProfileAction(null)}
          onConfirm={() => void confirmProfileAction()}
          pending={profileActionPending}
          profile={profileAction.profile}
          t={t}
        />
      )}
    </main>
  );
}
