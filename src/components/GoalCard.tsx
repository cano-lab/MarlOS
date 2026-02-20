import { Component, Show } from "solid-js";
import type { Goal } from "./PlanSpace";

interface GoalCardProps {
  goal: Goal;
  selected: boolean;
  isExpanded?: boolean;
  onSelect: (suid: string) => void;
  onDragStart: (e: MouseEvent, suid: string) => void;
  onDoubleClick: (suid: string) => void;
  onToggleExpand?: (suid: string) => void;
}

const GoalCard: Component<GoalCardProps> = (props) => {
  const energyIcons: Record<string, string> = {
    low: "\u{1F50B}",
    medium: "\u26A1",
    high: "\u{1F525}",
    flow: "\u{1F30A}",
  };

  const getMeaningStars = (score: number) => {
    const filled = Math.round(score / 2);
    return "\u2605".repeat(filled) + "\u2606".repeat(5 - filled);
  };

  const milestoneCount = () => (props.goal.milestones || []).length;
  const isExpanded = () => props.isExpanded ?? false;

  const handleExpandClick = (e: MouseEvent) => {
    e.stopPropagation();
    props.onToggleExpand?.(props.goal.suid);
  };

  return (
    <div
      class={`goal-card ${props.selected ? "selected" : ""} ${isExpanded() ? "expanded" : ""}`}
      style={{
        left: `${props.goal.position.x}px`,
        top: `${props.goal.position.y}px`,
      }}
      onMouseDown={(e) => {
        e.stopPropagation();
        props.onSelect(props.goal.suid);
        props.onDragStart(e, props.goal.suid);
      }}
      onDblClick={() => props.onDoubleClick(props.goal.suid)}
    >
      <div class="goal-card-header">
        <h3 class="goal-card-title">{props.goal.title}</h3>
        <Show when={milestoneCount() > 0 && props.onToggleExpand}>
          <button
            class={`goal-expand-btn ${isExpanded() ? "expanded" : ""}`}
            onClick={handleExpandClick}
            title={isExpanded() ? "Collapse steps" : "Expand steps"}
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M3 4.5L6 7.5L9 4.5"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        </Show>
      </div>

      <Show when={props.goal.description && !isExpanded()}>
        <p class="goal-card-description">{props.goal.description}</p>
      </Show>

      <div class="goal-progress">
        <div class="goal-progress-bar">
          <div
            class="goal-progress-fill"
            style={{ width: `${props.goal.progress}%` }}
          />
        </div>
        <span class="goal-progress-text">
          {isExpanded() ? (
            `${milestoneCount()} steps`
          ) : (
            `${props.goal.progress}%`
          )}
        </span>
      </div>

      <Show when={!isExpanded()}>
        <span class={`goal-energy ${props.goal.energyRequired}`}>
          <span class="goal-energy-icon">{energyIcons[props.goal.energyRequired]}</span>
          {props.goal.energyRequired}
        </span>

        <Show when={props.goal.tags.length > 0 || props.goal.meaningScore > 0}>
          <div class="goal-card-footer">
            <div class="goal-tags">
              {props.goal.tags.slice(0, 3).map((tag) => (
                <span class="goal-tag">{tag}</span>
              ))}
            </div>
            <Show when={props.goal.meaningScore > 0}>
              <div class="goal-meaning">
                <span class="goal-meaning-stars">{getMeaningStars(props.goal.meaningScore)}</span>
              </div>
            </Show>
          </div>
        </Show>
      </Show>

      <Show when={isExpanded() && milestoneCount() > 0}>
        <div class="goal-steps-indicator">
          <span class="goal-steps-count">{milestoneCount()} steps expanded</span>
        </div>
      </Show>
    </div>
  );
};

export default GoalCard;
