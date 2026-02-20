import { Component, createSignal, onMount, onCleanup, Show, For } from "solid-js";
import type { Widget } from "./PlanSpace";
import "./ClockWidget.css";

interface ClockWidgetProps {
  widget: Widget;
  onUpdate: (updates: Partial<Widget>) => void;
}

const ClockWidget: Component<ClockWidgetProps> = (props) => {
  const [currentTime, setCurrentTime] = createSignal(new Date());
  const [clockType, setClockTypeInternal] = createSignal<"digital" | "analog">("digital");
  const [showSeconds, setShowSeconds] = createSignal(true);
  const [use24Hour, setUse24Hour] = createSignal(true);

  onMount(() => {
    if (props.widget.data.type === "clock") {
      setClockTypeInternal(props.widget.data.clockType || "digital");
      setShowSeconds(props.widget.data.showSeconds !== false);
      setUse24Hour(props.widget.data.use24Hour !== false);
    }

    const timer = setInterval(() => {
      setCurrentTime(new Date());
    }, 1000);

    onCleanup(() => {
      clearInterval(timer);
    });
  });

  const setClockType = (type: "digital" | "analog") => {
    setClockTypeInternal(type);
    saveSettings({ clockType: type });
  };

  const toggleSeconds = () => {
    const newValue = !showSeconds();
    setShowSeconds(newValue);
    saveSettings({ showSeconds: newValue });
  };

  const toggle24Hour = () => {
    const newValue = !use24Hour();
    setUse24Hour(newValue);
    saveSettings({ use24Hour: newValue });
  };

  const saveSettings = (settings: Record<string, any>) => {
    const updates: Partial<Widget> = {
      data: {
        type: "clock",
        clockType: clockType(),
        showSeconds: showSeconds(),
        use24Hour: use24Hour(),
        ...settings,
      },
      lastTouched: new Date(),
    };
    props.onUpdate(updates);
  };

  const formatTime = () => {
    const date = currentTime();
    let hours = date.getHours();
    const minutes = date.getMinutes();
    const seconds = date.getSeconds();

    if (!use24Hour()) {
      const ampm = hours >= 12 ? "PM" : "AM";
      hours = hours % 12 || 12;
      return showSeconds()
        ? `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")} ${ampm}`
        : `${hours}:${String(minutes).padStart(2, "0")} ${ampm}`;
    }

    return showSeconds()
      ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
      : `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
  };

  const formatDate = () => {
    const date = currentTime();
    return date.toLocaleDateString("en-US", {
      weekday: "long",
      month: "short",
      day: "numeric"
    });
  };

  const getAnalogClockPositions = () => {
    const date = currentTime();
    const hours = date.getHours() % 12;
    const minutes = date.getMinutes();
    const seconds = date.getSeconds();

    const secondDeg = seconds * 6;
    const minuteDeg = minutes * 6 + seconds * 0.1;
    const hourDeg = (hours * 30) + (minutes * 0.5);

    return { secondDeg, minuteDeg, hourDeg };
  };

  return (
    <div class="clock-widget">
      <Show when={clockType() === "digital"}>
        <div class="clock-digital">
          <div class="clock-time" class:clock-time-large={!showSeconds()}>
            {formatTime()}
          </div>
          <div class="clock-date">
            {formatDate()}
          </div>
        </div>
      </Show>

      <Show when={clockType() === "analog"}>
        <div class="clock-analog">
          <svg viewBox="0 0 200 200" class="clock-face">
            {/* Clock face circle */}
            <circle cx="100" cy="100" r="95" fill="white" stroke="currentColor" stroke-width="2"/>

            {/* Hour markers */}
            <For each={Array.from({ length: 12 }, (_, i) => i)}>
              {(i) => {
                const angle = (i * 30 - 90) * (Math.PI / 180);
                const x1 = 100 + 80 * Math.cos(angle);
                const y1 = 100 + 80 * Math.sin(angle);
                const x2 = 100 + 90 * Math.cos(angle);
                const y2 = 100 + 90 * Math.sin(angle);

                return (
                  <line
                    x1={x1} y1={y1} x2={x2} y2={y2}
                    stroke="currentColor"
                    stroke-width={i % 3 === 0 ? 3 : 1}
                    stroke-opacity="0.3"
                  />
                );
              }}
            </For>

            {/* Hour hand */}
            <line
              x1="100" y1="100"
              x2="100"
              y2="50"
              stroke="currentColor"
              stroke-width="4"
              stroke-linecap="round"
              transform={`rotate(${getAnalogClockPositions().hourDeg} 100 100)`}
            />

            {/* Minute hand */}
            <line
              x1="100" y1="100"
              x2="100"
              y2="30"
              stroke="currentColor"
              stroke-width="3"
              stroke-linecap="round"
              transform={`rotate(${getAnalogClockPositions().minuteDeg} 100 100)`}
            />

            {/* Second hand */}
            <Show when={showSeconds()}>
              <line
                x1="100" y1="100"
                x2="100"
                y2="25"
                stroke="currentColor"
                stroke-width="1"
                stroke-linecap="round"
                transform={`rotate(${getAnalogClockPositions().secondDeg} 100 100)`}
                style="color: #ef4444;"
              />
            </Show>

            {/* Center dot */}
            <circle cx="100" cy="100" r="4" fill="currentColor"/>
          </svg>

          <div class="clock-date">
            {formatDate()}
          </div>
        </div>
      </Show>

      {/* Clock settings */}
      <div class="clock-settings">
        <button
          class={`clock-type-btn ${clockType() === "digital" ? "active" : ""}`}
          onClick={() => setClockType("digital")}
          title="Digital clock"
        >
          Digital
        </button>
        <button
          class={`clock-type-btn ${clockType() === "analog" ? "active" : ""}`}
          onClick={() => setClockType("analog")}
          title="Analog clock"
        >
          Analog
        </button>
        <button
          class={`clock-option-btn ${showSeconds() ? "active" : ""}`}
          onClick={toggleSeconds}
          title="Toggle seconds"
        >
          {showSeconds() ? "Sec" : "Min"}
        </button>
        <button
          class={`clock-option-btn ${use24Hour() ? "active" : ""}`}
          onClick={toggle24Hour}
          title="Toggle 12/24 hour"
        >
          {use24Hour() ? "24h" : "12h"}
        </button>
      </div>
    </div>
  );
};

export default ClockWidget;
