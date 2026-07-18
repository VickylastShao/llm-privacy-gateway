import { useEffect, useRef, useState } from "react";
import type { Translator } from "../i18n";

type Props = {
  name: string;
  onDelete: () => void;
  onRotate: () => void;
  t: Translator;
};

export function OverflowMenu({ name, onDelete, onRotate, t }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  const close = (restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) queueMicrotask(() => trigger.current?.focus());
  };

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close(true);
      }
    };
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) close(false);
    };
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [open]);

  return (
    <div className="overflow" ref={root}>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={t("moreActions", { name })}
        className="icon-button"
        onClick={() => setOpen((value) => !value)}
        ref={trigger}
        type="button"
      >
        ···
      </button>
      {open && (
        <div className="overflow-menu" role="menu">
          <button
            onClick={() => {
              close(false);
              onRotate();
            }}
            role="menuitem"
            type="button"
          >
            {t("rotateLocalKey")}
          </button>
          <button
            className="danger-action"
            onClick={() => {
              close(false);
              onDelete();
            }}
            role="menuitem"
            type="button"
          >
            {t("deleteConnection")}
          </button>
        </div>
      )}
    </div>
  );
}
