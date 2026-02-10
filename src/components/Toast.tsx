import { Component, createSignal, For, createContext, useContext, JSX } from "solid-js";
import "./Toast.css";

export type ToastType = "success" | "error" | "warning" | "info";

export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  duration?: number;
}

interface ToastContextValue {
  showToast: (message: string, type: ToastType, duration?: number) => void;
}

const ToastContext = createContext<ToastContextValue>();

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within a ToastProvider");
  }
  return context;
}

interface ToastProviderProps {
  children: JSX.Element;
}

export const ToastProvider: Component<ToastProviderProps> = (props) => {
  const [toasts, setToasts] = createSignal<Toast[]>([]);

  const showToast = (message: string, type: ToastType, duration?: number) => {
    const id = crypto.randomUUID();

    // Default durations: success/info auto-dismiss at 4s, errors persist
    const defaultDuration = type === "error" ? undefined : 4000;
    const actualDuration = duration ?? defaultDuration;

    const toast: Toast = { id, message, type, duration: actualDuration };
    setToasts((prev) => [...prev, toast]);

    if (actualDuration) {
      setTimeout(() => {
        removeToast(id);
      }, actualDuration);
    }
  };

  const removeToast = (id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  };

  return (
    <ToastContext.Provider value={{ showToast }}>
      {props.children}
      <div class="toast-container">
        <For each={toasts()}>
          {(toast) => (
            <ToastItem toast={toast} onDismiss={() => removeToast(toast.id)} />
          )}
        </For>
      </div>
    </ToastContext.Provider>
  );
};

interface ToastItemProps {
  toast: Toast;
  onDismiss: () => void;
}

const ToastItem: Component<ToastItemProps> = (props) => {
  const [isLeaving, setIsLeaving] = createSignal(false);

  const handleDismiss = () => {
    setIsLeaving(true);
    setTimeout(() => {
      props.onDismiss();
    }, 200); // Match animation duration
  };

  const getIcon = () => {
    switch (props.toast.type) {
      case "success":
        return (
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
            <circle cx="9" cy="9" r="8" stroke="currentColor" stroke-width="1.5" />
            <path d="M5.5 9.5L7.5 11.5L12.5 6.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        );
      case "error":
        return (
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
            <circle cx="9" cy="9" r="8" stroke="currentColor" stroke-width="1.5" />
            <path d="M6 6L12 12M12 6L6 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        );
      case "warning":
        return (
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
            <path d="M9 2L16 15H2L9 2Z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
            <path d="M9 7V10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            <circle cx="9" cy="12.5" r="0.75" fill="currentColor" />
          </svg>
        );
      case "info":
        return (
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
            <circle cx="9" cy="9" r="8" stroke="currentColor" stroke-width="1.5" />
            <path d="M9 8V13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            <circle cx="9" cy="5.5" r="0.75" fill="currentColor" />
          </svg>
        );
    }
  };

  return (
    <div
      class={`toast toast-${props.toast.type} ${isLeaving() ? "toast-leaving" : ""}`}
      role="alert"
    >
      <span class="toast-icon">{getIcon()}</span>
      <span class="toast-message">{props.toast.message}</span>
      <button class="toast-dismiss" onClick={handleDismiss} aria-label="Dismiss">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M3 3L11 11M11 3L3 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
    </div>
  );
};

export default ToastProvider;
