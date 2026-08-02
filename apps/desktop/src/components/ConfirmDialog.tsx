import { useEffect, useRef, type ReactNode } from "react";
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

type ModalDialogProps = {
  children: ReactNode;
  labelledBy: string;
  onCancel: () => void;
};

export function ModalDialog({ children, labelledBy, onCancel }: ModalDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const onCancelRef = useRef(onCancel);
  onCancelRef.current = onCancel;

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (typeof dialog.showModal === "function") {
      if (!dialog.open) dialog.showModal();
    } else {
      dialog.setAttribute("open", "");
    }
    dialog.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onCancelRef.current();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      if (dialog.open && typeof dialog.close === "function") dialog.close();
      else dialog.removeAttribute("open");
    };
  }, []);

  return (
    <dialog
      aria-labelledby={labelledBy}
      aria-modal="true"
      className="confirm-dialog"
      onCancel={(event) => {
        event.preventDefault();
        onCancelRef.current();
      }}
      ref={dialogRef}
    >
      {children}
    </dialog>
  );
}

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
    <ModalDialog labelledBy="confirm-title" onCancel={onCancel}>
      <h2 id="confirm-title">{title}</h2>
      <p>{description}</p>
      {blocked && <p className="blocked-message">{t("stopBeforeChange")}</p>}
      <div className="dialog-actions">
        <button data-autofocus disabled={pending} onClick={onCancel} type="button">
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
    </ModalDialog>
  );
}
