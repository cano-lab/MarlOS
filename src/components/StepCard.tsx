import { Component, For, Show, createSignal } from "solid-js";
import type { Milestone, AgentType, Complexity } from "./PlanSpace";
import "./StepCard.css";

interface StepCardProps {
  milestone: Milestone;
  parentGoalSuid: string;
  position: { x: number; y: number };
  onToggleSubtask: (milestoneId: string, subtaskId: string) => void;
  onUpdateMilestone: (milestone: Milestone) => void;
  onLaunchAgent?: (milestoneId: string, agentType: AgentType) => void;
  onDragStart?: (e: MouseEvent, milestoneId: string) => void;
  onAssignAgent?: (milestoneId: string) => void;
}

const agentIcons: Record<AgentType, string> = {
  research: "\u{1F50D}", // magnifying glass
  code: "\u{1F4BB}", // laptop
  image: "\u{1F3A8}", // palette
  audio: "\u{1F3B5}", // music note
  write: "\u{270F}\uFE0F", // pencil
  analyze: "\u{1F4CA}", // chart
  learn: "\u{1F4DA}", // books
  none: "",
};

const agentLabels: Record<AgentType, string> = {
  research: "Research",
  code: "Code",
  image: "Image",
  audio: "Audio",
  write: "Write",
  analyze: "Analyze",
  learn: "Learn",
  none: "None",
};

const complexityColors: Record<Complexity, string> = {
  trivial: "#68d391",
  simple: "#9ae6b4",
  moderate: "#f6ad55",
  complex: "#fc8181",
  epic: "#e53e3e",
};

const agentOptions: AgentType[] = ["research", "code", "write", "learn", "analyze", "image", "audio"];

const StepCard: Component<StepCardProps> = (props) => {
  const [showAgentPicker, setShowAgentPicker] = createSignal(false);

  const completedCount = () => {
    const subtasks = props.milestone.subtasks || [];
    return subtasks.filter((s) => s.completed).length;
  };

  const totalCount = () => {
    return (props.milestone.subtasks || []).length;
  };

  const progressPercent = () => {
    const total = totalCount();
    if (total === 0) return props.milestone.completed ? 100 : 0;
    return Math.round((completedCount() / total) * 100);
  };

  const agentType = () => props.milestone.agentType || "none";
  const complexity = () => props.milestone.complexity || "moderate";

  const handleLaunchAgent = (e: MouseEvent) => {
    e.stopPropagation();
    if (props.onLaunchAgent && agentType() !== "none") {
      props.onLaunchAgent(props.milestone.id, agentType());
    }
  };

  const handleSelectAgent = (e: MouseEvent, agent: AgentType) => {
    e.stopPropagation();
    props.onUpdateMilestone({ ...props.milestone, agentType: agent });
    setShowAgentPicker(false);
  };

  const handleToggleAgentPicker = (e: MouseEvent) => {
    e.stopPropagation();
    setShowAgentPicker(!showAgentPicker());
  };

  const handleMouseDown = (e: MouseEvent) => {
    // Only start drag on left click and not on interactive elements
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('button, input, select, .step-card-checkbox, .step-subtask-checkbox')) return;

    e.stopPropagation();
    props.onDragStart?.(e, props.milestone.id);
  };

  return (
    <div
      class={`step-card ${props.milestone.completed ? "completed" : ""}`}
      style={{
        left: `${props.position.x}px`,
        top: `${props.position.y}px`,
        "--complexity-color": complexityColors[complexity()],
      }}
      onMouseDown={handleMouseDown}
    >
      {/* Complexity indicator bar */}
      <div class="step-card-complexity" title={`Complexity: ${complexity()}`} />

      <div class="step-card-header">
        <div
          class={`step-card-checkbox ${props.milestone.completed ? "checked" : ""}`}
          onClick={() => props.onUpdateMilestone({ ...props.milestone, completed: !props.milestone.completed })}
        >
          <Show when={props.milestone.completed}>
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
              <path d="M2 6l3 3 5-5" stroke="white" stroke-width="2" fill="none" />
            </svg>
          </Show>
        </div>
        <h4 class="step-card-title">{props.milestone.title}</h4>
      </div>

      {/* Branch indicator */}
      <Show when={props.milestone.branch}>
        <div class="step-card-branch">
          <span class="step-branch-label">{props.milestone.branch}</span>
        </div>
      </Show>

      <Show when={props.milestone.notes}>
        <p class="step-card-notes">{props.milestone.notes}</p>
      </Show>

      <Show when={totalCount() > 0}>
        <div class="step-card-subtasks">
          <For each={props.milestone.subtasks}>
            {(subtask) => (
              <div class="step-subtask-item">
                <div
                  class={`step-subtask-checkbox ${subtask.completed ? "checked" : ""}`}
                  onClick={() => props.onToggleSubtask(props.milestone.id, subtask.id)}
                >
                  <Show when={subtask.completed}>
                    <svg width="8" height="8" viewBox="0 0 12 12" fill="none">
                      <path d="M2 6l3 3 5-5" stroke="white" stroke-width="2" fill="none" />
                    </svg>
                  </Show>
                </div>
                <span class={`step-subtask-text ${subtask.completed ? "completed" : ""}`}>
                  {subtask.title}
                </span>
              </div>
            )}
          </For>
        </div>
      </Show>

      {/* Agent section */}
      <div class="step-card-agent">
        <Show when={showAgentPicker()}>
          <div class="step-agent-picker">
            <For each={agentOptions}>
              {(agent) => (
                <button
                  class={`step-agent-option ${agent}`}
                  onClick={(e) => handleSelectAgent(e, agent)}
                  title={agentLabels[agent]}
                >
                  <span class="step-agent-icon">{agentIcons[agent]}</span>
                  <span>{agentLabels[agent]}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
        <Show when={!showAgentPicker()}>
          <Show
            when={agentType() !== "none"}
            fallback={
              <button class="step-agent-assign" title="Assign an agent" onClick={handleToggleAgentPicker}>
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                  <path d="M6 2v8M2 6h8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                </svg>
                <span>Assign Agent</span>
              </button>
            }
          >
            <div class="step-agent-row">
              <button
                class={`step-agent-launch ${agentType()}`}
                onClick={handleLaunchAgent}
                title={`Launch ${agentLabels[agentType()]} Agent`}
              >
                <span class="step-agent-icon">{agentIcons[agentType()]}</span>
                <span class="step-agent-label">{agentLabels[agentType()]}</span>
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none" class="step-agent-arrow">
                  <path d="M2 8L8 2M8 2H3M8 2v5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </button>
              <button class="step-agent-change" onClick={handleToggleAgentPicker} title="Change agent">
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                  <path d="M2 5h6M5 2l3 3-3 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </button>
            </div>
          </Show>
        </Show>
      </div>

      <Show when={totalCount() > 0}>
        <div class="step-card-progress">
          <div class="step-progress-bar">
            <div
              class="step-progress-fill"
              style={{ width: `${progressPercent()}%` }}
            />
          </div>
          <span class="step-progress-text">
            {completedCount()}/{totalCount()}
          </span>
        </div>
      </Show>
    </div>
  );
};

export default StepCard;
