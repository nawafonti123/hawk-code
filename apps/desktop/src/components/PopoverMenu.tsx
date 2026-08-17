import {
  type CSSProperties,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

interface PopoverMenuProps {
  label: string;
  trigger: ReactNode;
  children: (close: () => void) => ReactNode;
  placement?: "top-start" | "top-end" | "bottom-start" | "bottom-end";
  className?: string;
}

const VIEWPORT_GUTTER = 10;
const PANEL_GAP = 8;

export function PopoverMenu({
  label,
  trigger,
  children,
  placement = "bottom-start",
  className = "",
}: PopoverMenuProps) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<CSSProperties>({ opacity: 0 });
  const id = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const close = useCallback(() => setOpen(false), []);

  const updatePosition = useCallback(() => {
    const triggerElement = rootRef.current;
    const panel = panelRef.current;
    if (!triggerElement || !panel) return;

    const triggerRect = triggerElement.getBoundingClientRect();
    const panelRect = panel.getBoundingClientRect();
    const rtl = document.documentElement.dir === "rtl";
    const alignStart = placement.endsWith("start");
    const alignLeft = rtl ? !alignStart : alignStart;
    const requestedLeft = alignLeft
      ? triggerRect.left
      : triggerRect.right - panelRect.width;
    const maxLeft = Math.max(
      VIEWPORT_GUTTER,
      window.innerWidth - panelRect.width - VIEWPORT_GUTTER,
    );
    const left = Math.min(Math.max(requestedLeft, VIEWPORT_GUTTER), maxLeft);
    const requestedTop = placement.startsWith("top")
      ? triggerRect.top - panelRect.height - PANEL_GAP
      : triggerRect.bottom + PANEL_GAP;
    const maxTop = Math.max(
      VIEWPORT_GUTTER,
      window.innerHeight - panelRect.height - VIEWPORT_GUTTER,
    );
    const top = Math.min(Math.max(requestedTop, VIEWPORT_GUTTER), maxTop);

    setPosition({ left, top, opacity: 1 });
  }, [placement]);

  useLayoutEffect(() => {
    if (!open) return;
    updatePosition();
  }, [open, updatePosition]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !rootRef.current?.contains(target) &&
        !panelRef.current?.contains(target)
      ) {
        close();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [close, open, updatePosition]);

  return (
    <div className={`popover-root ${className}`} ref={rootRef}>
      <button
        type="button"
        className="popover-trigger"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen((value) => !value)}
      >
        {trigger}
      </button>
      {open
        ? createPortal(
            <div
              ref={panelRef}
              id={id}
              className="popover-panel popover-panel--portal"
              data-placement={placement}
              role="menu"
              aria-label={label}
              style={position}
            >
              {children(close)}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
