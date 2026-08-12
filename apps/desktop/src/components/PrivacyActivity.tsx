import { useState } from "react";
import type { Locale, TranslationKey, Translator } from "../i18n";
import type {
  PrivacyActivityItem,
  PrivacyActivityResponse,
  ProfileSummary,
} from "../model";

const categoryKeys: Record<string, TranslationKey> = {
  api_key: "categoryApiKey",
  private_key: "categoryPrivateKey",
  password: "categoryPassword",
  connection_string: "categoryConnectionString",
  jwt: "categoryJwt",
  china_resident_id: "categoryChinaResidentId",
  unified_social_credit_code: "categorySocialCreditCode",
  bank_card: "categoryBankCard",
  phone: "categoryPhone",
  email: "categoryEmail",
  ipv4: "categoryIpv4",
  ipv6: "categoryIpv6",
  mac_address: "categoryMacAddress",
  internal_hostname: "categoryInternalHostname",
  coordinate: "categoryCoordinate",
  custom: "categoryCustom",
  uninspected_content: "categoryUninspectedContent",
};

const actionKeys: Record<string, TranslationKey> = {
  block: "actionBlocked",
  pseudonymize: "actionPseudonymized",
  mask: "actionMasked",
  monitor: "actionMonitored",
  warn: "actionWarning",
};

function primaryAction(actions: string[]): string {
  return ["block", "warn", "pseudonymize", "mask", "monitor"].find((item) =>
    actions.includes(item),
  ) ?? "monitor";
}

function categoryName(kind: string, t: Translator): string {
  const key = categoryKeys[kind];
  return key ? t(key) : kind;
}

function actionName(action: string, t: Translator): string {
  const key = actionKeys[action];
  return key ? t(key) : action;
}

function formatTime(value: number, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));
}

function ActivityDetails({
  activity,
  profileName,
  t,
}: {
  activity: PrivacyActivityItem;
  profileName: string;
  t: Translator;
}) {
  return (
    <div className="activity-details">
      <dl>
        <div>
          <dt>{t("activityConnection")}</dt>
          <dd>{profileName}</dd>
        </div>
        <div>
          <dt>{t("activityEndpoint")}</dt>
          <dd><code>{activity.endpoint}</code></dd>
        </div>
        <div>
          <dt>{t("activityStatus")}</dt>
          <dd>{activity.status}</dd>
        </div>
        <div>
          <dt>{t("activityLatency")}</dt>
          <dd>{t("activityMilliseconds", { value: String(activity.latency_ms) })}</dd>
        </div>
        <div>
          <dt>{t("activityActions")}</dt>
          <dd className="activity-actions">
            {activity.actions.map((action) => (
              <span className={`activity-action activity-action--${action}`} key={action}>
                {actionName(action, t)}
              </span>
            ))}
          </dd>
        </div>
        <div>
          <dt>{t("activityRequestId")}</dt>
          <dd><code>{activity.request_id}</code></dd>
        </div>
      </dl>
      <div className="activity-preview">
        <strong>
          {activity.category_counts.uninspected_content
            ? t("activityCoverageDetail")
            : t("activitySafePreview")}
        </strong>
        {activity.preview_available ? (
          activity.previews.length > 0 ? (
            <>
              <ul>
                {activity.previews.map((preview, index) => (
                  <li key={`${preview.kind}-${preview.display}-${index}`}>
                    <span>{categoryName(preview.kind, t)}</span>
                    <code>{preview.display}</code>
                  </li>
                ))}
              </ul>
              <p>{t("activityPreviewSessionOnly")}</p>
            </>
          ) : (
            <p>{t("activityNoSafePreview")}</p>
          )
        ) : (
          <p>{t("activityPreviewExpired")}</p>
        )}
      </div>
    </div>
  );
}

export function PrivacyActivity({
  data,
  error,
  limit,
  locale,
  onShowMore,
  profiles,
  t,
}: {
  data: PrivacyActivityResponse | null;
  error: string | null;
  limit: number;
  locale: Locale;
  onShowMore: () => void;
  profiles: ProfileSummary[];
  t: Translator;
}) {
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const profileNames = new Map(profiles.map((profile) => [profile.id, profile.name]));

  const setOpen = (id: string, open: boolean) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  return (
    <section aria-labelledby="privacy-activity-title" className="privacy-activity">
      <div className="section-heading activity-heading">
        <div>
          <h2 id="privacy-activity-title">{t("activityTitle")}</h2>
          <p>{t("activityDescription")}</p>
        </div>
        {data && (
          <span className="activity-retention">
            {t("activityRetention", { hours: String(data.retention_hours) })}
          </span>
        )}
      </div>

      {error && <p className="activity-error" role="alert">{error}</p>}
      {data?.persist_dropped_count ? (
        <p className="activity-warning" role="status">
          {t("activityPersistenceWarning", {
            count: String(data.persist_dropped_count),
          })}
        </p>
      ) : null}
      {data && (
        <p className="activity-summary">
          {t("activitySummary", {
            count: String(data.total_activity_count),
            previews: String(data.session_preview_count),
          })}
        </p>
      )}

      {data && data.activities.length === 0 ? (
        <div className="activity-empty">
          <strong>{t("activityEmptyTitle")}</strong>
          <p>{t("activityEmptyDescription")}</p>
        </div>
      ) : (
        <div aria-label={t("activityList")} className="activity-list" role="list">
          {data?.activities.map((activity) => {
            const action = primaryAction(activity.actions);
            const profileName = profileNames.get(activity.profile_id) ?? t("activityUnknownProfile");
            return (
              <details
                key={activity.id}
                onToggle={(event) => setOpen(activity.id, event.currentTarget.open)}
                open={expanded.has(activity.id)}
                role="listitem"
              >
                <summary>
                  <span className="activity-time">{formatTime(activity.occurred_at_ms, locale)}</span>
                  <span className="activity-model">
                    <strong>{profileName}</strong>
                    {activity.model && <small>{activity.model}</small>}
                  </span>
                  <span className={`activity-action activity-action--${action}`}>
                    {actionName(action, t)}
                  </span>
                  <span className="activity-categories">
                    {Object.entries(activity.category_counts).map(([kind, count]) => (
                      <span className="activity-category" key={kind}>
                        {categoryName(kind, t)} ×{count}
                      </span>
                    ))}
                  </span>
                </summary>
                <ActivityDetails
                  activity={activity}
                  profileName={profileName}
                  t={t}
                />
              </details>
            );
          })}
        </div>
      )}

      {data && data.activities.length < data.total_activity_count && limit < 100 && (
        <button className="activity-more" onClick={onShowMore} type="button">
          {t("activityShowMore")}
        </button>
      )}
    </section>
  );
}
