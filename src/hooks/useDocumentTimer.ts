import { createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export function useDocumentTimer() {
  // Simple separate signals - much easier for SolidJS reactivity
  const [documentPath, setDocumentPath] = createSignal<string | null>(null);
  const [seconds, setSeconds] = createSignal(0);

  let intervalId: number | null = null;
  let syncIntervalId: number | null = null;

  const startTimer = (path: string) => {
    // Stop any existing timer first
    stopTimer();

    setDocumentPath(path);
    setSeconds(0);

    // Simple interval that just increments the counter
    intervalId = window.setInterval(() => {
      setSeconds(s => s + 1);
    }, 1000);

    // Sync to session every 60 seconds
    syncIntervalId = window.setInterval(() => {
      syncToSession();
    }, 60000);
  };

  const stopTimer = async () => {
    const path = documentPath();
    const elapsed = seconds();

    // Clear intervals
    if (intervalId !== null) {
      window.clearInterval(intervalId);
      intervalId = null;
    }
    if (syncIntervalId !== null) {
      window.clearInterval(syncIntervalId);
      syncIntervalId = null;
    }

    // Record final duration to session if we have a document
    if (path && elapsed > 0) {
      await recordDuration(path, elapsed);
    }

    // Reset state
    setDocumentPath(null);
    setSeconds(0);
  };

  const syncToSession = async () => {
    const path = documentPath();
    const elapsed = seconds();
    if (path && elapsed > 0) {
      await recordDuration(path, elapsed);
    }
  };

  const recordDuration = async (path: string, durationSecs: number) => {
    try {
      const filename = path.split(/[/\\]/).pop() || path;
      await invoke("record_session_activity", {
        activityType: "document_view",
        details: {
          title: filename,
          path: path,
          durationSecs: durationSecs,
        },
      });
    } catch (e) {
      console.debug("Failed to record document duration:", e);
    }
  };

  const formatTime = (totalSeconds: number): string => {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const secs = totalSeconds % 60;

    if (hours > 0) {
      return `${hours}:${minutes.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
    }
    return `${minutes}:${secs.toString().padStart(2, "0")}`;
  };

  // Cleanup on unmount
  onMount(() => {
    onCleanup(() => {
      stopTimer();
    });
  });

  return {
    documentPath,
    seconds,
    startTimer,
    stopTimer,
    formatTime,
    // Convenience getter for formatted time - call as formattedTime()
    formattedTime: () => formatTime(seconds()),
  };
}
