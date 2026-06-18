import { Component, Show, createSignal } from "solid-js";
import type { Widget } from "./PlanSpace";
import NoteWidget from "./NoteWidget";
import CalendarWidget from "./CalendarWidget";
import WeatherWidget from "./WeatherWidget";
import ClockWidget from "./ClockWidget";
import FileWidget from "./FileWidget";
import "./WidgetCard.css";

interface WidgetCardProps {
  widget: Widget;
  selected: boolean;
  onSelect: (id: string) => void;
  onDragStart: (e: MouseEvent, id: string) => void;
  onDoubleClick: (id: string) => void;
  onUpdate: (widget: Widget) => void;
  onDelete: (id: string) => void;
  onResizeStart?: (e: MouseEvent, id: string) => void;
}

const widgetIcons: Record<string, string> = {
  note: "\u{1F4DD}",  // 📝
  weather: "\u{1F324}",  // 🌤
  calendar: "\u{1F4C5}",  // 📅
  clock: "\u{1F552}",  // 🕐
  file: "\u{1F4C4}",  // 📄
  link: "\u{1F517}",  // 🔗
};

const WidgetCard: Component<WidgetCardProps> = (props) => {
  const [, setIsResizing] = createSignal(false);

  const getWidgetIcon = () => {
    return widgetIcons[props.widget.widgetType] || "\u{1F4DD}";
  };

  const handleDeleteClick = (e: MouseEvent) => {
    e.stopPropagation();
    if (confirm(`Delete "${props.widget.title}"?`)) {
      props.onDelete(props.widget.id);
    }
  };

  const handleResizeStart = (e: MouseEvent) => {
    e.stopPropagation();
    setIsResizing(true);
    props.onResizeStart?.(e, props.widget.id);
  };

  const getDefaultSize = () => {
    switch (props.widget.widgetType) {
      case "weather":
        return { width: 240, height: 320 };
      case "calendar":
        return { width: 320, height: 380 };
      case "file":
      case "clock":
        return { width: 180, height: 180 };
      case "link":
        return { width: 160, height: 80 };
      case "note":
      default:
        return props.widget.size || { width: 200, height: 200 };
    }
  };

  const size = () => {
    const defaultSize = getDefaultSize();
    return props.widget.size || defaultSize;
  };

  return (
    <div
      class={`widget-card widget-card-${props.widget.widgetType} ${props.selected ? "selected" : ""}`}
      style={{
        left: `${props.widget.position.x}px`,
        top: `${props.widget.position.y}px`,
        width: `${size().width}px`,
        height: `${size().height}px`,
      }}
      onMouseDown={(e) => {
        e.stopPropagation();
        props.onSelect(props.widget.id);
        props.onDragStart(e, props.widget.id);
      }}
      onDblClick={() => props.onDoubleClick(props.widget.id)}
    >
      <div class="widget-card-header">
        <div class="widget-card-header-left">
          <span class="widget-card-icon">{getWidgetIcon()}</span>
          <h3 class="widget-card-title">{props.widget.title}</h3>
        </div>
        <button
          class="widget-delete-btn"
          onClick={handleDeleteClick}
          title="Delete widget"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path
              d="M3 3l6 6M9 3l-6 6"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </div>

      <div class="widget-card-content">
        {/* Widget type-specific content */}
        <Show when={props.widget.widgetType === "note"}>
          <NoteWidget
            widget={props.widget}
            onUpdate={(updates) => {
              const updatedWidget: Widget = {
                ...props.widget,
                ...updates,
              };
              props.onUpdate(updatedWidget);
            }}
          />
        </Show>

        <Show when={props.widget.widgetType === "calendar"}>
          <CalendarWidget
            widget={props.widget}
            onUpdate={(updates) => {
              const updatedWidget: Widget = {
                ...props.widget,
                ...updates,
              };
              props.onUpdate(updatedWidget);
            }}
          />
        </Show>

        <Show when={props.widget.widgetType === "weather"}>
          <WeatherWidget
            widget={props.widget}
            onUpdate={(updates) => {
              const updatedWidget: Widget = {
                ...props.widget,
                ...updates,
              };
              props.onUpdate(updatedWidget);
            }}
          />
        </Show>

        <Show when={props.widget.widgetType === "clock"}>
          <ClockWidget
            widget={props.widget}
            onUpdate={(updates) => {
              const updatedWidget: Widget = {
                ...props.widget,
                ...updates,
              };
              props.onUpdate(updatedWidget);
            }}
          />
        </Show>

        <Show when={props.widget.widgetType === "file"}>
          <FileWidget
            widget={props.widget}
            onUpdate={(updates) => {
              const updatedWidget: Widget = {
                ...props.widget,
                ...updates,
              };
              props.onUpdate(updatedWidget);
            }}
          />
        </Show>

        <Show when={props.widget.widgetType === "link"}>
          <div class="widget-placeholder">
            {props.widget.widgetType} widget
            <div class="widget-placeholder-hint">Coming soon</div>
          </div>
        </Show>
      </div>

      {/* Resize handle */}
      <Show when={props.widget.widgetType === "note" || props.widget.widgetType === "calendar"}>
        <div
          class="widget-resize-handle"
          onMouseDown={handleResizeStart}
          title="Drag to resize"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            <path
              d="M6 4v6M4 6v6M2 8v6"
              stroke="currentColor"
              stroke-width="1.2"
              stroke-linecap="round"
            />
          </svg>
        </div>
      </Show>
    </div>
  );
};

export default WidgetCard;
