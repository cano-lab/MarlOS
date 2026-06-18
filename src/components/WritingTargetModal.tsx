import { Component, createSignal } from "solid-js";
import "./WritingTargetModal.css";

interface WritingTargetModalProps {
  currentTarget: number;
  currentWords: number;
  onSetTarget: (target: number) => void;
  onClose: () => void;
}

const PRESETS = [500, 1000, 2000, 3000, 5000, 10000];

const WritingTargetModal: Component<WritingTargetModalProps> = (props) => {
  const [value, setValue] = createSignal(props.currentTarget || 1000);

  const handleSubmit = (e: Event) => {
    e.preventDefault();
    props.onSetTarget(value());
    props.onClose();
  };

  const clearTarget = () => {
    props.onSetTarget(0);
    props.onClose();
  };

  return (
    <div class="wt-overlay" onClick={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="wt-modal">
        <h3>Set Writing Target</h3>
        <p class="wt-current">Current: {props.currentWords.toLocaleString()} words</p>

        <form onSubmit={handleSubmit}>
          <div class="wt-input-row">
            <label>Target word count:</label>
            <input
              type="number"
              min="0"
              step="100"
              value={value()}
              onInput={(e) => setValue(parseInt(e.currentTarget.value) || 0)}
              autofocus
            />
          </div>

          <div class="wt-presets">
            {PRESETS.map((preset) => (
              <button
                type="button"
                class="wt-preset"
                classList={{ "wt-preset-active": value() === preset }}
                onClick={() => setValue(preset)}
              >
                {preset >= 1000 ? `${preset / 1000}k` : preset}
              </button>
            ))}
          </div>

          <div class="wt-actions">
            <button type="button" class="wt-btn-clear" onClick={clearTarget}>Clear Target</button>
            <div class="wt-actions-right">
              <button type="button" class="wt-btn-cancel" onClick={props.onClose}>Cancel</button>
              <button type="submit" class="wt-btn-set">Set Target</button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
};

export default WritingTargetModal;
