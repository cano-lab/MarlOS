import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { Goal, Milestone, AgentType, Complexity } from "./PlanSpace";

interface MilestoneSuggestion {
  title: string;
  description: string | null;
}

interface GoalEditorProps {
  goal: Goal | null;
  isNew: boolean;
  onSave: (goal: Partial<Goal>) => void;
  onDelete?: () => void;
  onClose: () => void;
}

const GoalEditor: Component<GoalEditorProps> = (props) => {
  const [title, setTitle] = createSignal("");
  const [description, setDescription] = createSignal("");
  const [energyRequired, setEnergyRequired] = createSignal<Goal["energyRequired"]>("medium");
  const [meaningScore, setMeaningScore] = createSignal(5);
  const [excitement, setExcitement] = createSignal(5);
  const [tags, setTags] = createSignal("");
  const [milestones, setMilestones] = createSignal<Milestone[]>([]);
  const [newMilestone, setNewMilestone] = createSignal("");

  // Subtask input state for each milestone (keyed by milestone id)
  const [newSubtasks, setNewSubtasks] = createSignal<Record<string, string>>({});

  // Expanded milestones (for showing subtasks)
  const [expandedMilestones, setExpandedMilestones] = createSignal<Set<string>>(new Set());

  // AI generation state
  const [generating, setGenerating] = createSignal(false);
  const [aiError, setAiError] = createSignal<string | null>(null);

  // Initialize form when goal changes
  createEffect(() => {
    if (props.goal) {
      setTitle(props.goal.title);
      setDescription(props.goal.description);
      setEnergyRequired(props.goal.energyRequired);
      setMeaningScore(props.goal.meaningScore);
      setExcitement(props.goal.excitement);
      setTags(props.goal.tags.join(", "));
      setMilestones(props.goal.milestones || []);
      // Auto-expand milestones that have subtasks
      const expanded = new Set<string>();
      (props.goal.milestones || []).forEach((m) => {
        if (m.subtasks && m.subtasks.length > 0) {
          expanded.add(m.id);
        }
      });
      setExpandedMilestones(expanded);
    } else {
      // Reset for new goal
      setTitle("");
      setDescription("");
      setEnergyRequired("medium");
      setMeaningScore(5);
      setExcitement(5);
      setTags("");
      setMilestones([]);
      setNewSubtasks({});
      setExpandedMilestones(new Set<string>());
    }
  });

  const handleSave = () => {
    const tagList = tags()
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    props.onSave({
      title: title(),
      description: description(),
      energyRequired: energyRequired(),
      meaningScore: meaningScore(),
      excitement: excitement(),
      tags: tagList,
      milestones: milestones(),
    });
  };

  const addMilestone = () => {
    if (newMilestone().trim()) {
      const newId = crypto.randomUUID();
      setMilestones([
        ...milestones(),
        {
          id: newId,
          title: newMilestone().trim(),
          completed: false,
          subtasks: [],
        },
      ]);
      setNewMilestone("");
    }
  };

  const toggleMilestone = (id: string) => {
    setMilestones(
      milestones().map((m) =>
        m.id === id ? { ...m, completed: !m.completed } : m
      )
    );
  };

  const removeMilestone = (id: string) => {
    setMilestones(milestones().filter((m) => m.id !== id));
    // Clean up subtask input state
    const subtasks = { ...newSubtasks() };
    delete subtasks[id];
    setNewSubtasks(subtasks);
    // Clean up expanded state
    const expanded = new Set(expandedMilestones());
    expanded.delete(id);
    setExpandedMilestones(expanded);
  };

  const toggleMilestoneExpand = (id: string) => {
    const expanded = new Set(expandedMilestones());
    if (expanded.has(id)) {
      expanded.delete(id);
    } else {
      expanded.add(id);
    }
    setExpandedMilestones(expanded);
  };

  // Subtask management
  const addSubtask = (milestoneId: string) => {
    const subtaskText = newSubtasks()[milestoneId]?.trim();
    if (!subtaskText) return;

    setMilestones(
      milestones().map((m) => {
        if (m.id === milestoneId) {
          return {
            ...m,
            subtasks: [
              ...(m.subtasks || []),
              {
                id: crypto.randomUUID(),
                title: subtaskText,
                completed: false,
              },
            ],
          };
        }
        return m;
      })
    );

    // Clear the input
    setNewSubtasks({ ...newSubtasks(), [milestoneId]: "" });
  };

  const toggleSubtask = (milestoneId: string, subtaskId: string) => {
    setMilestones(
      milestones().map((m) => {
        if (m.id === milestoneId && m.subtasks) {
          return {
            ...m,
            subtasks: m.subtasks.map((s) =>
              s.id === subtaskId ? { ...s, completed: !s.completed } : s
            ),
          };
        }
        return m;
      })
    );
  };

  const removeSubtask = (milestoneId: string, subtaskId: string) => {
    setMilestones(
      milestones().map((m) => {
        if (m.id === milestoneId && m.subtasks) {
          return {
            ...m,
            subtasks: m.subtasks.filter((s) => s.id !== subtaskId),
          };
        }
        return m;
      })
    );
  };

  // Update a specific field on a milestone
  const updateMilestoneField = (milestoneId: string, field: keyof Milestone, value: any) => {
    setMilestones(
      milestones().map((m) => {
        if (m.id === milestoneId) {
          return { ...m, [field]: value };
        }
        return m;
      })
    );
  };

  const generateMilestones = async () => {
    if (!title().trim()) {
      setAiError("Please enter a goal title first");
      return;
    }

    setGenerating(true);
    setAiError(null);

    try {
      const suggestions = await invoke<MilestoneSuggestion[]>("plan_generate_milestones", {
        goalTitle: title(),
        goalDescription: description(),
        energyLevel: energyRequired(),
      });

      // Convert suggestions to milestones and add them
      const newMilestones: Milestone[] = suggestions.map((s) => ({
        id: crypto.randomUUID(),
        title: s.title,
        completed: false,
        notes: s.description || undefined,
        subtasks: [],
      }));

      setMilestones([...milestones(), ...newMilestones]);
    } catch (e) {
      console.error("Failed to generate milestones:", e);
      setAiError(typeof e === "string" ? e : "Failed to generate milestones. Make sure an AI provider is configured.");
    } finally {
      setGenerating(false);
    }
  };

  const energyOptions: { value: Goal["energyRequired"]; label: string; icon: string }[] = [
    { value: "low", label: "Low", icon: "\u{1F50B}" },
    { value: "medium", label: "Medium", icon: "\u26A1" },
    { value: "high", label: "High", icon: "\u{1F525}" },
    { value: "flow", label: "Flow", icon: "\u{1F30A}" },
  ];

  return (
    <div class="goal-editor-overlay" onClick={props.onClose}>
      <div class="goal-editor" onClick={(e) => e.stopPropagation()}>
        <div class="goal-editor-header">
          <h2 class="goal-editor-title">
            {props.isNew ? "New Goal" : "Edit Goal"}
          </h2>
          <button class="goal-editor-close" onClick={props.onClose}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
            </svg>
          </button>
        </div>

        <div class="goal-editor-body">
          <div class="goal-editor-field">
            <label class="goal-editor-label">Title</label>
            <input
              type="text"
              class="goal-editor-input"
              value={title()}
              onInput={(e) => setTitle(e.currentTarget.value)}
              placeholder="What do you want to achieve?"
              autofocus
            />
          </div>

          <div class="goal-editor-field">
            <label class="goal-editor-label">Description</label>
            <textarea
              class="goal-editor-input goal-editor-textarea"
              value={description()}
              onInput={(e) => setDescription(e.currentTarget.value)}
              placeholder="Add context, motivation, or details..."
            />
          </div>

          <div class="goal-editor-field">
            <label class="goal-editor-label">Energy Required</label>
            <div class="goal-editor-energy">
              <For each={energyOptions}>
                {(option) => (
                  <button
                    class={`goal-editor-energy-btn ${energyRequired() === option.value ? "active" : ""}`}
                    onClick={() => setEnergyRequired(option.value)}
                  >
                    <span>{option.icon}</span>
                    {option.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          <div class="goal-editor-field">
            <label class="goal-editor-label">Meaning Score</label>
            <div class="goal-editor-slider">
              <input
                type="range"
                min="1"
                max="10"
                value={meaningScore()}
                onInput={(e) => setMeaningScore(parseInt(e.currentTarget.value))}
              />
              <span class="goal-editor-slider-value">{meaningScore()}</span>
            </div>
          </div>

          <div class="goal-editor-field">
            <label class="goal-editor-label">Excitement</label>
            <div class="goal-editor-slider">
              <input
                type="range"
                min="1"
                max="10"
                value={excitement()}
                onInput={(e) => setExcitement(parseInt(e.currentTarget.value))}
              />
              <span class="goal-editor-slider-value">{excitement()}</span>
            </div>
          </div>

          <div class="goal-editor-field">
            <label class="goal-editor-label">Tags</label>
            <input
              type="text"
              class="goal-editor-input"
              value={tags()}
              onInput={(e) => setTags(e.currentTarget.value)}
              placeholder="project, learning, health (comma separated)"
            />
          </div>

          <div class="goal-editor-field">
            <div class="goal-editor-label-row">
              <label class="goal-editor-label">Milestones</label>
              <button
                class="plan-btn plan-btn-ai"
                onClick={generateMilestones}
                disabled={generating() || !title().trim()}
                title="Use AI to suggest milestones"
              >
                <Show when={generating()} fallback={
                  <>
                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                      <path d="M7 1v2M7 11v2M1 7h2M11 7h2M2.75 2.75l1.5 1.5M9.75 9.75l1.5 1.5M2.75 11.25l1.5-1.5M9.75 4.25l1.5-1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                    </svg>
                    AI Suggest
                  </>
                }>
                  <span class="ai-loading-spinner" />
                  Thinking...
                </Show>
              </button>
            </div>
            <Show when={aiError()}>
              <div class="goal-editor-error">{aiError()}</div>
            </Show>
            <div class="goal-editor-milestones">
              <For each={milestones()}>
                {(milestone) => (
                  <div class="goal-milestone-wrapper">
                    <div class="goal-milestone-item">
                      <button
                        class={`goal-milestone-expand ${expandedMilestones().has(milestone.id) ? "expanded" : ""}`}
                        onClick={() => toggleMilestoneExpand(milestone.id)}
                        title={expandedMilestones().has(milestone.id) ? "Collapse subtasks" : "Expand subtasks"}
                      >
                        <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                          <path
                            d="M2.5 3.5L5 6L7.5 3.5"
                            stroke="currentColor"
                            stroke-width="1.5"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          />
                        </svg>
                      </button>
                      <div
                        class={`goal-milestone-checkbox ${milestone.completed ? "checked" : ""}`}
                        onClick={() => toggleMilestone(milestone.id)}
                      >
                        {milestone.completed && (
                          <svg width="12" height="12" viewBox="0 0 12 12" fill="white">
                            <path d="M2 6l3 3 5-5" stroke="white" stroke-width="2" fill="none" />
                          </svg>
                        )}
                      </div>
                      <span class={`goal-milestone-text ${milestone.completed ? "completed" : ""}`}>
                        {milestone.title}
                      </span>
                      <Show when={(milestone.subtasks?.length || 0) > 0}>
                        <span class="goal-milestone-subtask-count">
                          {milestone.subtasks?.filter((s) => s.completed).length}/{milestone.subtasks?.length}
                        </span>
                      </Show>
                      <button
                        class="goal-editor-close"
                        onClick={() => removeMilestone(milestone.id)}
                        style={{ padding: "4px" }}
                      >
                        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                          <path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                        </svg>
                      </button>
                    </div>

                    {/* Subtasks section */}
                    <Show when={expandedMilestones().has(milestone.id)}>
                      <div class="goal-subtasks-section">
                        {/* Agent & Complexity selectors */}
                        <div class="milestone-config-row">
                          <div class="milestone-config-item">
                            <label class="milestone-config-label">Agent</label>
                            <select
                              class="milestone-config-select"
                              value={milestone.agentType || "none"}
                              onChange={(e) => updateMilestoneField(milestone.id, "agentType", e.currentTarget.value as AgentType)}
                            >
                              <option value="none">None</option>
                              <option value="research">Research</option>
                              <option value="code">Code</option>
                              <option value="write">Write</option>
                              <option value="learn">Learn</option>
                              <option value="image">Image</option>
                              <option value="audio">Audio</option>
                              <option value="analyze">Analyze</option>
                            </select>
                          </div>
                          <div class="milestone-config-item">
                            <label class="milestone-config-label">Complexity</label>
                            <select
                              class="milestone-config-select"
                              value={milestone.complexity || "moderate"}
                              onChange={(e) => updateMilestoneField(milestone.id, "complexity", e.currentTarget.value as Complexity)}
                            >
                              <option value="trivial">Trivial</option>
                              <option value="simple">Simple</option>
                              <option value="moderate">Moderate</option>
                              <option value="complex">Complex</option>
                              <option value="epic">Epic</option>
                            </select>
                          </div>
                        </div>
                        <div class="milestone-config-row">
                          <div class="milestone-config-item" style={{ flex: 1 }}>
                            <label class="milestone-config-label">Branch</label>
                            <input
                              type="text"
                              class="milestone-config-input"
                              value={milestone.branch || ""}
                              onInput={(e) => updateMilestoneField(milestone.id, "branch", e.currentTarget.value || undefined)}
                              placeholder="e.g., frontend, research, design..."
                            />
                          </div>
                        </div>

                        {/* Subtasks */}
                        <div class="milestone-subtasks-label">Subtasks</div>
                        <For each={milestone.subtasks || []}>
                          {(subtask) => (
                            <div class="goal-subtask-item">
                              <div
                                class={`goal-subtask-checkbox ${subtask.completed ? "checked" : ""}`}
                                onClick={() => toggleSubtask(milestone.id, subtask.id)}
                              >
                                {subtask.completed && (
                                  <svg width="8" height="8" viewBox="0 0 12 12" fill="white">
                                    <path d="M2 6l3 3 5-5" stroke="white" stroke-width="2" fill="none" />
                                  </svg>
                                )}
                              </div>
                              <span class={`goal-subtask-text ${subtask.completed ? "completed" : ""}`}>
                                {subtask.title}
                              </span>
                              <button
                                class="goal-editor-close"
                                onClick={() => removeSubtask(milestone.id, subtask.id)}
                                style={{ padding: "2px" }}
                              >
                                <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
                                  <path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                                </svg>
                              </button>
                            </div>
                          )}
                        </For>
                        <div class="goal-subtask-add">
                          <input
                            type="text"
                            class="goal-subtask-input"
                            value={newSubtasks()[milestone.id] || ""}
                            onInput={(e) =>
                              setNewSubtasks({ ...newSubtasks(), [milestone.id]: e.currentTarget.value })
                            }
                            onKeyDown={(e) => e.key === "Enter" && addSubtask(milestone.id)}
                            placeholder="Add subtask..."
                          />
                          <button
                            class="plan-btn"
                            onClick={() => addSubtask(milestone.id)}
                            disabled={!(newSubtasks()[milestone.id]?.trim())}
                            style={{ padding: "2px 6px", "font-size": "11px" }}
                          >
                            +
                          </button>
                        </div>
                      </div>
                    </Show>
                  </div>
                )}
              </For>
              <div class="goal-milestone-item" style={{ background: "transparent", border: "1px dashed var(--plan-border)" }}>
                <input
                  type="text"
                  class="goal-editor-input"
                  style={{ background: "transparent", border: "none", padding: "0", "margin-left": "28px" }}
                  value={newMilestone()}
                  onInput={(e) => setNewMilestone(e.currentTarget.value)}
                  onKeyDown={(e) => e.key === "Enter" && addMilestone()}
                  placeholder="Add milestone..."
                />
                <button
                  class="plan-btn"
                  onClick={addMilestone}
                  disabled={!newMilestone().trim()}
                  style={{ padding: "4px 8px" }}
                >
                  +
                </button>
              </div>
            </div>
          </div>
        </div>

        <div class="goal-editor-footer">
          {!props.isNew && props.onDelete && (
            <button
              class="plan-btn"
              onClick={props.onDelete}
              style={{ "margin-right": "auto", color: "#c53030" }}
            >
              Delete
            </button>
          )}
          <button class="plan-btn" onClick={props.onClose}>
            Cancel
          </button>
          <button
            class="plan-btn plan-btn-primary"
            onClick={handleSave}
            disabled={!title().trim()}
          >
            {props.isNew ? "Create Goal" : "Save Changes"}
          </button>
        </div>
      </div>
    </div>
  );
};

export default GoalEditor;
