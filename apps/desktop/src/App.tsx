import { useEffect, useState } from "react";
import { desktopApi, type DesktopApi } from "./desktop-api";
import {
  initialLocale,
  setLocalePreference,
  translate,
  type Locale,
} from "./i18n";
import type { GatewayStatus, ProfileSummary } from "./model";

export function App({
  api = desktopApi,
  initialLanguage,
}: {
  api?: DesktopApi;
  initialLanguage?: string;
}) {
  const [locale, setLocale] = useState<Locale>(() =>
    initialLocale(initialLanguage),
  );
  const [status, setStatus] = useState<GatewayStatus>({ running: false });
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);

  const changeLocale = (nextLocale: Locale) => {
    setLocale(nextLocale);
    setLocalePreference(nextLocale);
  };

  useEffect(() => {
    void api.gatewayStatus().then(setStatus).catch(() => undefined);
    void api.listProfiles().then(setProfiles).catch(() => undefined);
  }, [api]);

  return (
    <main lang={locale}>
      <header>
        <div>
          <h1>{t("appName")}</h1>
          <p>{t("appPurpose")}</p>
        </div>
        <div aria-label={t("language")} role="group">
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
        <button disabled={profiles.length === 0} type="button">
          {status.running ? t("stopProtection") : t("startProtection")}
        </button>
      </header>
      <section aria-labelledby="first-run-title">
        <h2 id="first-run-title">{t("firstRunTitle")}</h2>
        <p>{t("firstRunDescription")}</p>
        <ol>
          <li>{t("stepAdd")}</li>
          <li>{t("stepCopy")}</li>
          <li>{t("stepStart")}</li>
        </ol>
        <form aria-label={t("connectionForm")}>
          <label>
            {t("connectionName")}
            <input name="name" />
          </label>
        </form>
      </section>
    </main>
  );
}
