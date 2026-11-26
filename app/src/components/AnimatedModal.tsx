import React, { useEffect, useState } from "react";
import { AlertTriangle, X } from "lucide-react";

export type PresencePhase = "closed" | "entering" | "entered" | "exiting";

export function useAnimatedPresence(open: boolean, exitDurationMs = 190): {
  mounted: boolean;
  phase: PresencePhase;
} {
  const [mounted, setMounted] = useState(open);
  const [phase, setPhase] = useState<PresencePhase>(open ? "entering" : "closed");

  if (open && !mounted) {
    setMounted(true);
    setPhase("entering");
  }

  useEffect(() => {
    let raf1: number | null = null;
    let raf2: number | null = null;
    let timer: number | null = null;

    if (open) {
      raf1 = window.requestAnimationFrame(() => {
        raf2 = window.requestAnimationFrame(() => {
          setPhase("entered");
        });
      });
    } else {
      setPhase((prev) => (prev === "closed" ? "closed" : "exiting"));
      timer = window.setTimeout(() => {
        setMounted(false);
        setPhase("closed");
      }, exitDurationMs);
    }

    return () => {
      if (raf1 !== null) window.cancelAnimationFrame(raf1);
      if (raf2 !== null) window.cancelAnimationFrame(raf2);
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [open, exitDurationMs]);

  return { mounted, phase };
}

interface AnimatedModalProps {
  open: boolean;
  onClose: () => void;
  panelClassName?: string;
  children: React.ReactNode;
}

export const AnimatedModal: React.FC<AnimatedModalProps> = ({
  open,
  onClose,
  panelClassName = "max-w-md",
  children,
}) => {
  const { mounted, phase } = useAnimatedPresence(open, 190);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  if (!mounted) return null;

  const isVisible = phase === "entered";

  return (
    <div
      role="dialog"
      aria-modal="true"
      data-state={isVisible ? "open" : "closed"}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      className="modal-backdrop fixed inset-0 z-50 flex items-center justify-center p-4"
    >
      <div
        data-state={isVisible ? "open" : "closed"}
        onClick={(e) => e.stopPropagation()}
        className={`modal-panel glass-card relative w-full rounded-2xl border border-white/12 p-5 shadow-2xl ${panelClassName}`}
      >
        {children}
      </div>
    </div>
  );
};

interface ConfirmModalProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export const ConfirmModal: React.FC<ConfirmModalProps> = ({
  open,
  title,
  message,
  confirmLabel = "确认",
  cancelLabel = "取消",
  danger = false,
  onConfirm,
  onClose,
}) => {
  const [cached, setCached] = useState({ title, message });

  useEffect(() => {
    if (open) {
      setCached({ title, message });
    }
  }, [open, title, message]);

  return (
    <AnimatedModal open={open} onClose={onClose} panelClassName="max-w-sm">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-2 text-[14px] font-semibold text-zinc-100">
          {danger && <AlertTriangle className="h-4 w-4 text-rose-400" />}
          {cached.title}
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-lg p-1 text-zinc-400 transition hover:bg-white/8 hover:text-zinc-100"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
      <p className="text-[12.5px] leading-relaxed text-zinc-300">{cached.message}</p>
      <div className="mt-5 flex justify-end gap-2">
        <button
          type="button"
          onClick={onClose}
          className="rounded-lg border border-white/10 px-3.5 py-1.5 text-[12px] text-zinc-300 transition hover:bg-white/6"
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          onClick={() => {
            onConfirm();
            onClose();
          }}
          className={
            danger
              ? "rounded-lg bg-rose-500 px-3.5 py-1.5 text-[12px] font-medium text-white transition hover:bg-rose-400"
              : "rounded-lg bg-emerald-500 px-3.5 py-1.5 text-[12px] font-medium text-zinc-950 transition hover:bg-emerald-400"
          }
        >
          {confirmLabel}
        </button>
      </div>
    </AnimatedModal>
  );
};
