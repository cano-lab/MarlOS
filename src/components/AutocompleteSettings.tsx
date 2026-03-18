import { Component, createSignal, createEffect } from "solid-js";
import { 
  autocompleteService, 
  ContentType 
} from "../../services/autocomplete-service";
import "./AutocompleteSettings.css";

interface AutocompleteSettingsProps {
  onClose: () => void;
}

interface AutocompleteConfig {
  enabled: boolean;
  triggerDelay: number;
  maxSuggestionLength: number;
  enabledTypes: Record<ContentType, boolean>;
  conservativeMode: boolean;
}

const defaultConfig: AutocompleteConfig = {
  enabled: true,
  triggerDelay: 600,
  maxSuggestionLength: 8,
  enabledTypes: {
    code: true,
    prose: true,
    academic: true,
    markdown: true,
    unknown: true,
  },
  conservativeMode: true,
};

const AutocompleteSettings: Component<AutocompleteSettingsProps> = (props) => {
  const [config, setConfig] = createSignal<AutocompleteConfig>(defaultConfig);
  const [saved, setSaved] = createSignal(false);

  createEffect(() => {
    // Load saved config
    const saved = localStorage.getItem('marlos-autocomplete-config');
    if (saved) {
      try {
        setConfig({ ...defaultConfig, ...JSON.parse(saved) });
      } catch (e) {
        console.error('Failed to load autocomplete config:', e);
      }
    }
  });

  const saveConfig = () => {
    localStorage.setItem('marlos-autocomplete-config', JSON.stringify(config()));
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  const updateConfig = (updates: Partial<AutocompleteConfig>) => {
    setConfig(prev => ({ ...prev, ...updates }));
    // Auto-save
    setTimeout(saveConfig, 100);
  };

  const updateTypeEnabled = (type: ContentType, enabled: boolean) => {
    setConfig(prev => ({
      ...prev,
      enabledTypes: { ...prev.enabledTypes, [type]: enabled },
    }));
    setTimeout(saveConfig, 100);
  };

  return (
    <div class="autocomplete-settings-overlay" onClick={(e) => {
      if (e.target === e.currentTarget) props.onClose();
    }}>
      <div class="autocomplete-settings-modal">
        <div class="settings-header">
          <h2>✨ Autocomplete Settings</h2>
          <button class="close-btn" onClick={props.onClose}>&times;</button>
        </div>

        <div class="settings-content">
          <!-- Enable/Disable -->
          <section class="setting-section">
            <label class="toggle-label">
              <input
                type="checkbox"
                checked={config().enabled}
                onChange={(e) => updateConfig({ enabled: e.currentTarget.checked })}
              />
              <span class="toggle-slider"></span>
              <span class="toggle-text">Enable Autocomplete</span>
            </label>
            <p class="setting-description">
              Show AI-powered suggestions as you type
            </p>
          </section>

          <!-- Conservative Mode -->
          <section class="setting-section">
            <label class="toggle-label">
              <input
                type="checkbox"
                checked={config().conservativeMode}
                onChange={(e) => updateConfig({ conservativeMode: e.currentTarget.checked })}
              />
              <span class="toggle-slider"></span>
              <span class="toggle-text">Conservative Mode</span>
            </label>
            <p class="setting-description">
              Only suggest after longer pauses. Fewer interruptions, more deliberate suggestions.
            </p>
          </section>

          <!-- Trigger Delay -->
          <section class="setting-section">
            <label class="range-label">Trigger Delay</label>
            <div class="range-control">
              <input
                type="range"
                min="300"
                max="1500"
                step="100"
                value={config().triggerDelay}
                onInput={(e) => updateConfig({ triggerDelay: parseInt(e.currentTarget.value) })}
              />
              <span class="range-value">{config().triggerDelay}ms</span>
            </div>
            <p class="setting-description">
              How long to wait after you stop typing before showing suggestions
            </p>
          </section>

          <!-- Max Length -->
          <section class="setting-section">
            <label class="range-label">Max Suggestion Length</label>
            <div class="range-control">
              <input
                type="range"
                min="3"
                max="15"
                step="1"
                value={config().maxSuggestionLength}
                onInput={(e) => updateConfig({ maxSuggestionLength: parseInt(e.currentTarget.value) })}
              />
              <span class="range-value">{config().maxSuggestionLength} words</span>
            </div>
            <p class="setting-description">
              Short suggestions feel more like assistance, longer ones feel like replacement.
              We recommend 6-8 words for the best experience.
            </p>
          </section>

          <!-- Content Type Toggles -->
          <section class="setting-section">
            <h3>Enable for Content Types</h3>
            <div class="type-toggles">
              {([
                { type: 'code', label: '💻 Code', desc: 'Functions, variables, expressions' },
                { type: 'prose', label: '📝 Prose', desc: 'General writing, notes' },
                { type: 'academic', label: '🎓 Academic', desc: 'Papers, citations, research' },
                { type: 'markdown', label: '⬇️ Markdown', desc: 'Formatting, links, lists' },
              ] as { type: ContentType; label: string; desc: string }[]).map(({ type, label, desc }) => (
                <label class="type-toggle">
                  <input
                    type="checkbox"
                    checked={config().enabledTypes[type]}
                    onChange={(e) => updateTypeEnabled(type, e.currentTarget.checked)}
                  />
                  <div class="type-info">
                    <span class="type-label">{label}</span>
                    <span class="type-desc">{desc}</span>
                  </div>
                </label>
              ))}
            </div>
          </section>

          <!-- Keyboard Shortcuts -->
          <section class="setting-section">
            <h3>Keyboard Shortcuts</h3>
            <div class="shortcuts-list">
              <div class="shortcut">
                <kbd>Tab</kbd>
                <span>Accept full suggestion</span>
              </div>
              <div class="shortcut">
                <kbd>Ctrl</kbd> + <kbd>→</kbd>
                <span>Accept one word</span>
              </div>
              <div class="shortcut">
                <kbd>Esc</kbd>
                <span>Dismiss suggestion</span>
              </div>
            </div>
          </section>
        </div>

        <div class="settings-footer">
          {saved() && <span class="save-indicator">✓ Saved</span>}
          <button class="btn-close" onClick={props.onClose}>Done</button>
        </div>
      </div>
    </div>
  );
};

export default AutocompleteSettings;
