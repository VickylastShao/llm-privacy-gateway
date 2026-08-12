import { useCallback, useEffect, useRef, useState } from "react";
import type { Translator } from "../i18n";

type Props = {
  name: string;
  onDelete: () => void;
  onEdit: () => void;
  onRotate: () => void;
  t: Translator;
};

export function OverflowMenu({ name, onDelete, onEdit, onRotate, t }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  const close = useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) queueMicrotask(() => trigger.current?.focus());
  }, []);

  useEffect(() => {
    if (!open) return;
    queueMicrotask(() => {
      root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
    });
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close(true);
        return;
      }
      if (event.key === "Tab") {
        close(false);
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      const items = Array.from(
        root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [],
      );
      if (items.length === 0) return;
      event.preventDefault();
      const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
      let nextIndex = 0;
      if (event.key === "End") nextIndex = items.length - 1;
      if (event.key === "ArrowDown") nextIndex = (currentIndex + 1 + items.length) % items.length;
      if (event.key === "ArrowUp") {
        nextIndex = (currentIndex - 1 + items.length) % items.length;
      }
      items[nextIndex]?.focus();
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
  }, [close, open]);

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
              onEdit();
            }}
            role="menuitem"
            type="button"
          >
            {t("editConnection")}
          </button>
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
