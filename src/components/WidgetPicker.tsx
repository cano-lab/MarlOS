import { Component, Show } from "solid-js";
import type { WidgetType } from "./PlanSpace";
import "./WidgetPicker.css";

interface WidgetPickerProps {
  show: boolean;
  onClose: () => void;
  onSelectWidgetType: (type: WidgetType) => void;
}

interface WidgetOption {
  type: WidgetType;
  icon: string;
  name: string;
  description: string;
  priority: number;
}

const widgetOptions: WidgetOption[] = [
  {
    type: "note",
    icon: "\u{1F4DD}",  // 📝
    name: "Note",
    description: "Quick notes and thoughts with markdown support",
    priority: 1,
  },
  {
    type: "weather",
    icon: "\u{1F324}",  // 🌤
    name: "Weather",
    description: "Current weather & 3-day forecast for your location",
    priority: 2,
  },
  {
    type: "calendar",
    icon: "\u{1F4C5}",  // 📅
    name: "Calendar",
    description: "Mini calendar with events tracking",
    priority: 3,
  },
  {
    type: "clock",
    icon: "\u{1F552}",  // 🕐
    name: "Clock",
    description: "Digital or analog clock with date display",
    priority: 4,
  },
  {
    type: "file",
    icon: "\u{1F4C4}",  // 📄
    name: "File",
    description: "Link local files and images with preview",
    priority: 5,
  },
  {
    type: "link",
    icon: "\u{1F517}",  // 🔗
    name: "Link",
    description: "Quick access to web resources",
    priority: 6,
  },
];

const WidgetPicker: Component<WidgetPickerProps> = (props) => {
  const handleSelectWidget = (type: WidgetType) => {
    props.onSelectWidgetType(type);
    props.onClose();
  };

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === e.currentTarget) {
      props.onClose();
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onClose();
    }
  };

  return (
    <Show when={props.show}>
      <div
        class="widget-picker-backdrop"
        onClick={handleBackdropClick}
        onKeyDown={handleKeyDown}
      >
        <div class="widget-picker-modal">
          <div class="widget-picker-header">
            <h2 class="widget-picker-title">Add Widget</h2>
            <button
              class="widget-picker-close"
              onClick={props.onClose}
              title="Close (Esc)"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path
                  d="M4 4l8 8M12 4l-8 8"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                />
              </svg>
            </button>
          </div>

          <div class="widget-picker-grid">
            {widgetOptions.map((option) => (
              <button
                class="widget-picker-option"
                onClick={() => handleSelectWidget(option.type)}
              >
                <span class="widget-picker-option-icon">{option.icon}</span>
                <div class="widget-picker-option-content">
                  <h3 class="widget-picker-option-name">{option.name}</h3>
                  <p class="widget-picker-option-description">{option.description}</p>
                </div>
              </button>
            ))}
          </div>

          <div class="widget-picker-footer">
            <p class="widget-picker-hint">
              Widgets coexist with goals on your canvas
            </p>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default WidgetPicker;
