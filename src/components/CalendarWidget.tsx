import { Component, createSignal, onMount, For, Show } from "solid-js";
import type { Widget } from "./PlanSpace";
import "./CalendarWidget.css";

interface CalendarEvent {
  date: string; // YYYY-MM-DD
  title: string;
}

interface CalendarWidgetProps {
  widget: Widget;
  onUpdate: (updates: Partial<Widget>) => void;
}

const CalendarWidget: Component<CalendarWidgetProps> = (props) => {
  const [viewingMonth, setViewingMonth] = createSignal(new Date());
  const [selectedDate, setSelectedDate] = createSignal<string | null>(null);
  const [eventTitle, setEventTitle] = createSignal("");
  const [events, setEvents] = createSignal<CalendarEvent[]>([]);

  onMount(() => {
    if (props.widget.data.type === "calendar") {
      const savedEvents = props.widget.data.events || [];
      setEvents(savedEvents);
    }
  });

  const saveEvents = (newEvents: CalendarEvent[]) => {
    setEvents(newEvents);
    const updates: Partial<Widget> = {
      data: {
        type: "calendar",
        showWeekends: true,
        events: newEvents,
      },
      lastTouched: new Date(),
    };
    props.onUpdate(updates);
  };

  const getDaysInMonth = (date: Date) => {
    const year = date.getFullYear();
    const month = date.getMonth();
    const firstDay = new Date(year, month, 1);
    const lastDay = new Date(year, month + 1, 0);
    const daysInMonth = lastDay.getDate();
    const startDayOfWeek = firstDay.getDay(); // 0 = Sunday

    const days: (Date | null)[] = [];

    // Add empty cells for days before the first of the month
    for (let i = 0; i < startDayOfWeek; i++) {
      days.push(null);
    }

    // Add days of the month
    for (let i = 1; i <= daysInMonth; i++) {
      days.push(new Date(year, month, i));
    }

    return days;
  };

  const isToday = (date: Date) => {
    const today = new Date();
    return (
      date.getDate() === today.getDate() &&
      date.getMonth() === today.getMonth() &&
      date.getFullYear() === today.getFullYear()
    );
  };

  const hasEvent = (date: Date) => {
    const dateStr = formatDate(date);
    return events().some((e) => e.date === dateStr);
  };

  const formatDate = (date: Date) => {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  };

  const prevMonth = () => {
    const newDate = new Date(viewingMonth());
    newDate.setMonth(newDate.getMonth() - 1);
    setViewingMonth(newDate);
  };

  const nextMonth = () => {
    const newDate = new Date(viewingMonth());
    newDate.setMonth(newDate.getMonth() + 1);
    setViewingMonth(newDate);
  };

  const handleDateClick = (date: Date) => {
    const dateStr = formatDate(date);
    setSelectedDate(dateStr);

    // Find existing event
    const existingEvent = events().find((e) => e.date === dateStr);
    setEventTitle(existingEvent?.title || "");
  };

  const saveEvent = () => {
    if (!selectedDate()) return;

    const newEvents = events().filter((e) => e.date !== selectedDate());

    if (eventTitle().trim()) {
      newEvents.push({
        date: selectedDate()!,
        title: eventTitle().trim(),
      });
    }

    saveEvents(newEvents);
    setSelectedDate(null);
    setEventTitle("");
  };

  const cancelEvent = () => {
    setSelectedDate(null);
    setEventTitle("");
  };

  const weekDays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const monthNames = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December"
  ];

  const days = () => getDaysInMonth(viewingMonth());

  return (
    <div class="calendar-widget">
      <div class="calendar-header">
        <button class="calendar-nav-btn" onClick={prevMonth} title="Previous month">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M8 2L4 6L8 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>
        <h3 class="calendar-month">
          {monthNames[viewingMonth().getMonth()]} {viewingMonth().getFullYear()}
        </h3>
        <button class="calendar-nav-btn" onClick={nextMonth} title="Next month">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M4 2L8 6L4 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>
      </div>

      <div class="calendar-weekdays">
        <For each={weekDays}>
          {(day) => <div class="calendar-weekday">{day}</div>}
        </For>
      </div>

      <div class="calendar-days">
        <For each={days()}>
          {(day) => (
            <div
              class={`calendar-day ${
                day ? (isToday(day) ? "today" : "") : "empty"
              } ${hasEvent(day!) ? "has-event" : ""} ${
                selectedDate() === formatDate(day!) ? "selected" : ""
              }`}
              onClick={() => day && handleDateClick(day)}
            >
              {day ? (
                <>
                  <span class="calendar-day-number">{day.getDate()}</span>
                  {hasEvent(day) && <span class="calendar-event-dot" />}
                </>
              ) : null}
            </div>
          )}
        </For>
      </div>

      {/* Event editor */}
      <Show when={selectedDate()}>
        <div class="calendar-event-editor">
          <h4 class="calendar-event-date">
            {new Date(selectedDate()!).toLocaleDateString("en-US", {
              weekday: "long",
              month: "short",
              day: "numeric"
            })}
          </h4>
          <input
            type="text"
            class="calendar-event-input"
            placeholder="Add event..."
            value={eventTitle()}
            onInput={(e) => setEventTitle(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") saveEvent();
              if (e.key === "Escape") cancelEvent();
            }}
            autofocus
          />
          <div class="calendar-event-actions">
            <button class="calendar-event-btn calendar-event-save" onClick={saveEvent}>
              Save
            </button>
            <button class="calendar-event-btn calendar-event-cancel" onClick={cancelEvent}>
              Cancel
            </button>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default CalendarWidget;
