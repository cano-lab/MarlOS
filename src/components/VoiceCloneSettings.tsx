import { Component, createSignal, Show, onCleanup } from "solid-js";
import { useVoiceClone } from "../services/voice-clone-service";
import "./VoiceCloneSettings.css";

const SUGGESTED_TRANSCRIPT =
  "The quick brown fox jumps over the lazy dog. " +
  "Pack my box with five dozen liquor jugs. " +
  "How vexingly quick daft zebras jump.";

const TARGET_RECORD_SECONDS = 12;

const VoiceCloneSettings: Component<{ onClose?: () => void }> = (props) => {
  const voice = useVoiceClone();
  const [recording, setRecording] = createSignal(false);
  const [recordedBlob, setRecordedBlob] = createSignal<Blob | null>(null);
  const [recordSeconds, setRecordSeconds] = createSignal(0);
  const [transcript, setTranscript] = createSignal(SUGGESTED_TRANSCRIPT);
  const [previewUrl, setPreviewUrl] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [testText, setTestText] = createSignal("Hello, this is my cloned voice speaking through MarlOS.");
  const [synthBusy, setSynthBusy] = createSignal(false);

  let mediaRecorder: MediaRecorder | null = null;
  let recordedChunks: Blob[] = [];
  let timerId: number | null = null;

  const startRecording = async () => {
    setError(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      recordedChunks = [];
      mediaRecorder = new MediaRecorder(stream);
      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) recordedChunks.push(e.data);
      };
      mediaRecorder.onstop = () => {
        stream.getTracks().forEach((t) => t.stop());
        const blob = new Blob(recordedChunks, { type: mediaRecorder?.mimeType || "audio/webm" });
        setRecordedBlob(blob);
        if (previewUrl()) URL.revokeObjectURL(previewUrl()!);
        setPreviewUrl(URL.createObjectURL(blob));
      };
      mediaRecorder.start();
      setRecording(true);
      setRecordSeconds(0);
      timerId = window.setInterval(() => {
        setRecordSeconds((s) => s + 1);
        if (recordSeconds() >= TARGET_RECORD_SECONDS + 5) stopRecording();
      }, 1000);
    } catch (e) {
      setError(`Microphone error: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const stopRecording = () => {
    if (mediaRecorder && mediaRecorder.state !== "inactive") {
      mediaRecorder.stop();
    }
    if (timerId !== null) {
      clearInterval(timerId);
      timerId = null;
    }
    setRecording(false);
  };

  const saveReference = async () => {
    const blob = recordedBlob();
    if (!blob) {
      setError("No recording yet");
      return;
    }
    if (!transcript().trim()) {
      setError("Transcript cannot be empty");
      return;
    }
    try {
      setError(null);
      await voice.setReferenceFromBlob(blob, transcript());
    } catch (e) {
      setError(`Failed to save reference: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const clearReference = async () => {
    try {
      await voice.clearReference();
      setRecordedBlob(null);
      if (previewUrl()) {
        URL.revokeObjectURL(previewUrl()!);
        setPreviewUrl(null);
      }
    } catch (e) {
      setError(`Failed to clear: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const testSynthesis = async () => {
    setSynthBusy(true);
    setError(null);
    try {
      await voice.speak(testText());
    } catch (e) {
      setError(`Synthesis failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSynthBusy(false);
    }
  };

  onCleanup(() => {
    if (timerId !== null) clearInterval(timerId);
    if (previewUrl()) URL.revokeObjectURL(previewUrl()!);
    if (mediaRecorder && mediaRecorder.state !== "inactive") mediaRecorder.stop();
  });

  return (
    <div class="voice-clone-settings">
      <div class="vcs-header">
        <h2>Voice Cloning (F5-TTS)</h2>
        <Show when={props.onClose}>
          <button class="vcs-close" onClick={props.onClose}>✕</button>
        </Show>
      </div>

      <Show when={voice.status()}>
        <div class={`vcs-status ${voice.status()!.has_reference ? "ok" : "warn"}`}>
          <Show
            when={voice.status()!.has_reference}
            fallback={<span>No voice reference set — record one below.</span>}
          >
            <span>✓ Voice reference ready</span>
            <button class="vcs-link" onClick={clearReference} disabled={voice.busy()}>
              Clear
            </button>
          </Show>
          <Show when={voice.status()!.python_path}>
            <span class="vcs-meta">Python: {voice.status()!.python_path}</span>
          </Show>
        </div>
      </Show>

      <section class="vcs-section">
        <h3>1. Record reference audio</h3>
        <p class="vcs-help">
          Record about {TARGET_RECORD_SECONDS} seconds of yourself reading the transcript below
          (or your own text — just keep them matching). Find a quiet room; clean audio matters.
        </p>

        <div class="vcs-record-row">
          <Show
            when={!recording()}
            fallback={
              <button class="vcs-btn vcs-btn-stop" onClick={stopRecording}>
                ■ Stop ({recordSeconds()}s)
              </button>
            }
          >
            <button class="vcs-btn vcs-btn-record" onClick={startRecording}>
              ● Start recording
            </button>
          </Show>

          <Show when={previewUrl()}>
            <audio controls src={previewUrl()!} class="vcs-audio" />
          </Show>
        </div>

        <Show when={recording()}>
          <div class="vcs-progress">
            <div
              class="vcs-progress-bar"
              style={{
                width: `${Math.min(100, (recordSeconds() / TARGET_RECORD_SECONDS) * 100)}%`,
              }}
            />
          </div>
        </Show>
      </section>

      <section class="vcs-section">
        <h3>2. Transcript</h3>
        <p class="vcs-help">
          Exactly what you said in the recording. F5-TTS uses this to learn the mapping
          between your voice and text.
        </p>
        <textarea
          class="vcs-transcript"
          rows="3"
          value={transcript()}
          onInput={(e) => setTranscript(e.currentTarget.value)}
        />
      </section>

      <div class="vcs-actions">
        <button
          class="vcs-btn vcs-btn-primary"
          onClick={saveReference}
          disabled={voice.busy() || !recordedBlob() || !transcript().trim()}
        >
          {voice.busy() ? "Saving..." : "Save reference"}
        </button>
      </div>

      <section class="vcs-section">
        <h3>Voice tuning</h3>
        <p class="vcs-help">
          Affects every synthesis — used by the chat speak button too. Stored locally.
        </p>
        <div class="vcs-controls">
          <label class="vcs-control">
            <span class="vcs-control-label">
              Speed <span class="vcs-control-value">{voice.settings().speed.toFixed(2)}×</span>
            </span>
            <input
              type="range"
              min="0.5"
              max="1.5"
              step="0.05"
              value={voice.settings().speed}
              onInput={(e) =>
                voice.updateSettings({ speed: parseFloat(e.currentTarget.value) })
              }
            />
            <span class="vcs-control-hint">0.5× slow ↔ 1.5× fast</span>
          </label>

          <label class="vcs-control">
            <span class="vcs-control-label">Quality</span>
            <select
              class="vcs-select"
              value={voice.settings().nfe}
              onChange={(e) =>
                voice.updateSettings({ nfe: parseInt(e.currentTarget.value, 10) })
              }
            >
              <option value="16">Fast (nfe 16) — quickest, lower fidelity</option>
              <option value="32">Balanced (nfe 32) — recommended</option>
              <option value="64">High (nfe 64) — slowest, best fidelity</option>
            </select>
            <span class="vcs-control-hint">
              Higher quality runs the diffusion sampler for more steps.
            </span>
          </label>
        </div>
      </section>

      <Show when={voice.status()?.has_reference}>
        <section class="vcs-section">
          <h3>Test it</h3>
          <textarea
            class="vcs-test-input"
            rows="2"
            value={testText()}
            onInput={(e) => setTestText(e.currentTarget.value)}
          />
          <button
            class="vcs-btn vcs-btn-primary"
            onClick={testSynthesis}
            disabled={synthBusy() || voice.busy() || !testText().trim()}
          >
            {synthBusy() ? "Synthesizing... (first call ~30s)" : "🔊 Speak in my voice"}
          </button>
        </section>
      </Show>

      <Show when={error()}>
        <div class="vcs-error">{error()}</div>
      </Show>
    </div>
  );
};

export default VoiceCloneSettings;
