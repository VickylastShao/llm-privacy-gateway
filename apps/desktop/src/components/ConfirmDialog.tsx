import type { Translator } from "../i18n";
import type { ProfileSummary } from "../model";

export type ProfileAction = "rotate" | "delete";

type Props = {
  action: ProfileAction;
  blocked: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  pending: boolean;
  profile: ProfileSummary;
  t: Translator;
};

export function ConfirmDialog({
  action,
  blocked,
  onCancel,
  onConfirm,
  pending,
  profile,
  t,
}: Props) {
  const title = action === "rotate" ? t("rotateTitle") : t("deleteTitle", { name: profile.name });
  const description = action === "rotate" ? t("rotateDescription") : t("deleteDescription");
  const confirm = action === "rotate" ? t("confirmRotate") : t("confirmDelete");

  return (
    <dialog aria-labelledby="confirm-title" className="confirm-dialog" open>
      <h2 id="confirm-title">{title}</h2>
      <p>{description}</p>
      {blocked && <p className="blocked-message">{t("stopBeforeChange")}</p>}
      <div className="dialog-actions">
        <button disabled={pending} onClick={onCancel} type="button">
          {t("cancel")}
        </button>
        <button
          className={action === "delete" ? "danger-button" : "primary-button"}
          disabled={blocked || pending}
          onClick={onConfirm}
          type="button"
        >
          {confirm}
        </button>
      </div>
    </dialog>
  );
}
