import { Component, createSignal, For, Show, onMount, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import GoalCard from "./GoalCard";
import GoalEditor from "./GoalEditor";
import StepCard from "./StepCard";
import WidgetCard from "./WidgetCard";
import WidgetPicker from "./WidgetPicker";
import LocationPicker, { SavedLocation } from "./LocationPicker";
import "./PlanSpace.css";

// Types
export type AgentType = "research" | "code" | "image" | "audio" | "write" | "analyze" | "learn" | "none";
export type Complexity = "trivial" | "simple" | "moderate" | "complex" | "epic";
export type WidgetType = "note" | "weather" | "calendar" | "file" | "link" | "clock";

// Export goal/milestone context as markdown for AI agents
export const exportGoalToMarkdown = (goal: Goal, milestone?: Milestone): string => {
  let md = `# Goal: ${goal.title}\n\n`;

  if (goal.description) {
    md += `## Description\n${goal.description}\n\n`;
  }

  md += `## Properties\n`;
  md += `- **Progress**: ${goal.progress}%\n`;
  md += `- **Energy Required**: ${goal.energyRequired}\n`;
  md += `- **Meaning Score**: ${goal.meaningScore}/10\n`;
  md += `- **Tags**: ${goal.tags.join(", ") || "none"}\n\n`;

  if (goal.milestones && goal.milestones.length > 0) {
    md += `## Milestones\n\n`;
    goal.milestones.forEach((m, i) => {
      const status = m.completed ? "[x]" : "[ ]";
      const current = milestone?.id === m.id ? " **<-- CURRENT FOCUS**" : "";
      md += `${i + 1}. ${status} **${m.title}**${current}\n`;
      if (m.complexity) md += `   - Complexity: ${m.complexity}\n`;
      if (m.branch) md += `   - Branch: ${m.branch}\n`;
      if (m.notes) md += `   - Notes: ${m.notes}\n`;
      if (m.subtasks && m.subtasks.length > 0) {
        m.subtasks.forEach((s) => {
          const sStatus = s.completed ? "[x]" : "[ ]";
          md += `   - ${sStatus} ${s.title}\n`;
        });
      }
      md += `\n`;
    });
  }

  if (milestone) {
    md += `## Current Task: ${milestone.title}\n\n`;
    if (milestone.notes) md += `${milestone.notes}\n\n`;
    md += `Please help me accomplish this milestone as part of the larger goal.\n`;
  }

  return md;
};

export interface Subtask {
  id: string;
  title: string;
  completed: boolean;
}

export interface CalendarEvent {
  date: string;
  title: string;
}

export interface Milestone {
  id: string;
  title: string;
  completed: boolean;
  notes?: string;
  subtasks?: Subtask[];
  // Agent & complexity fields
  agentType?: AgentType;
  complexity?: Complexity;
  branch?: string; // Group related milestones into branches
  dependsOn?: string[]; // IDs of milestones this one depends on
  // Position (relative to parent goal, set when dragged)
  position?: { x: number; y: number };
}

export interface Goal {
  suid: string;
  title: string;
  description: string;
  energyRequired: "low" | "medium" | "high" | "flow";
  currentEnergy?: "depleted" | "low" | "medium" | "high" | "peak";
  meaningScore: number;
  excitement: number;
  progressType: "tasks" | "milestones" | "energy-invested";
  progress: number;
  milestones?: Milestone[];
  position: { x: number; y: number };
  color?: string;
  relatedGoals: string[];
  blockedBy?: string[];
  tags: string[];
  createdAt: Date;
  lastTouched: Date;
}

// Widget system for Plan Space
export type WidgetData =
  | { type: "note"; content: string; color?: string }
  | { type: "weather"; location?: string; units: "celsius" | "fahrenheit" }
  | { type: "calendar"; showWeekends: boolean; events?: CalendarEvent[] }
  | { type: "file"; filePath?: string; isImage?: boolean }
  | { type: "link"; url: string; title: string }
  | { type: "clock"; clockType?: "digital" | "analog"; showSeconds?: boolean; use24Hour?: boolean };

export interface Widget {
  id: string;
  type: "widget";           // Discriminator for canvas items
  widgetType: WidgetType;       // Which kind of widget
  title: string;
  position: { x: number; y: number };
  size?: { width: number; height: number };  // Configurable size
  data: WidgetData;            // Widget-specific data
  createdAt: Date;
  lastTouched: Date;
}

// Unified canvas item type
export type CanvasItem = Goal | Widget;

interface PlanSpaceProps {
  onClose: () => void;
  onLaunchAgent?: (agentType: AgentType, context: string, milestone: Milestone, goal: Goal) => void;
}

// Calculate step card positions in a circle around the parent goal
const calculateStepPositions = (
  parentPos: { x: number; y: number },
  stepCount: number
): { x: number; y: number }[] => {
  if (stepCount === 0) return [];

  // Center of the goal card (assuming ~240px wide, ~150px tall)
  const centerX = parentPos.x + 120;
  const centerY = parentPos.y + 75;

  // Radius increases with more steps to avoid overlap
  const baseRadius = 180;
  const radius = baseRadius + Math.max(0, stepCount - 4) * 20;

  // Start from top-right and go clockwise, leaving space at top for the goal
  const startAngle = -Math.PI / 3; // -60 degrees (top right)
  const endAngle = Math.PI + Math.PI / 3; // 240 degrees (top left)
  const totalAngle = endAngle - startAngle;

  return Array.from({ length: stepCount }, (_, i) => {
    const angle = startAngle + (i / Math.max(1, stepCount - 1)) * totalAngle;
    // Offset to position top-left corner of step card (180px wide, ~100px tall)
    return {
      x: centerX + Math.cos(angle) * radius - 90,
      y: centerY + Math.sin(angle) * radius - 50,
    };
  });
};

const PlanSpace: Component<PlanSpaceProps> = (props) => {
  // Canvas state
  const [goals, setGoals] = createSignal<Goal[]>([]);
  const [widgets, setWidgets] = createSignal<Widget[]>([]);
  const [selectedGoal, setSelectedGoal] = createSignal<string | null>(null);
  const [selectedWidget, setSelectedWidget] = createSignal<string | null>(null);
  const [draggingGoal, setDraggingGoal] = createSignal<string | null>(null);
  const [draggingWidget, setDraggingWidget] = createSignal<string | null>(null);
  const [dragOffset, setDragOffset] = createSignal({ x: 0, y: 0 });

  // Step card dragging state
  const [draggingStep, setDraggingStep] = createSignal<{ goalSuid: string; milestoneId: string } | null>(null);
  const [stepDragOffset, setStepDragOffset] = createSignal({ x: 0, y: 0 });

  // Widget resizing state
  const [resizingWidget, setResizingWidget] = createSignal<string | null>(null);
  const [resizeStart, setResizeStart] = createSignal({ x: 0, y: 0, width: 0, height: 0 });

  // Expanded goals state
  const [expandedGoals, setExpandedGoals] = createSignal<Set<string>>(new Set());

  // Panning state
  const [isPanning, setIsPanning] = createSignal(false);
  const [panOffset, setPanOffset] = createSignal({ x: 0, y: 0 });
  const [panStart, setPanStart] = createSignal({ x: 0, y: 0 });

  // Zoom state
  const [zoom, setZoom] = createSignal(1);

  // UI state
  const [showEditor, setShowEditor] = createSignal(false);
  const [editingGoal, setEditingGoal] = createSignal<Goal | null>(null);
  const [isNewGoal, setIsNewGoal] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  const [showWidgetPicker, setShowWidgetPicker] = createSignal(false);

  // Saved locations
  interface SavedLocation {
    id: string;
    name: string;
    x: number;
    y: number;
    zoom: number;
    createdAt: Date;
  }
  const [savedLocations, setSavedLocations] = createSignal<SavedLocation[]>([]);
  const [showLocationPicker, setShowLocationPicker] = createSignal(false);

  // Current energy level (for filtering)
  const [currentEnergy, setCurrentEnergy] = createSignal(3);

  // Load goals on mount
  onMount(async () => {
    await loadGoals();
    await loadWidgets();
    loadSavedLocations();
    window.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    window.removeEventListener("keydown", handleKeyDown);
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      if (showEditor()) {
        setShowEditor(false);
      } else if (expandedGoals().size > 0) {
        // Collapse all expanded goals first
        setExpandedGoals(new Set<string>());
      } else {
        props.onClose();
      }
    }
    // Delete selected goal or widget
    if ((e.key === "Delete" || e.key === "Backspace") && !showEditor()) {
      if (selectedGoal()) {
        deleteGoal(selectedGoal()!);
      } else if (selectedWidget()) {
        deleteWidget(selectedWidget()!);
      }
    }
  };

  const loadGoals = async () => {
    setLoading(true);
    try {
      const result = await invoke<Goal[]>("plan_list_goals");
      setGoals(result);
    } catch (e) {
      console.error("Failed to load goals:", e);
      setGoals([]);
    } finally {
      setLoading(false);
    }
  };

  const loadWidgets = async () => {
    try {
      const result = await invoke<any[]>("plan_list_widgets");
      const mappedWidgets: Widget[] = result.map((w: any) => ({
        id: w.id,
        type: "widget",
        widgetType: w.widgetType,
        title: w.title,
        position: { x: w.position.x, y: w.position.y },
        size: w.size ? { width: w.size.width, height: w.size.height } : undefined,
        data: w.data,
        createdAt: new Date(w.createdAt),
        lastTouched: new Date(w.lastTouched),
      }));
      setWidgets(mappedWidgets);
    } catch (e) {
      console.error("Failed to load widgets:", e);
      setWidgets([]);
    }
  };

  const loadSavedLocations = () => {
    try {
      const stored = localStorage.getItem("plan-space-locations");
      if (stored) {
        const parsed = JSON.parse(stored);
        const locations: SavedLocation[] = parsed.map((l: any) => ({
          ...l,
          createdAt: new Date(l.createdAt),
        }));
        setSavedLocations(locations);
      }
    } catch (e) {
      console.error("Failed to load saved locations:", e);
    }
  };

  const saveCurrentLocation = (name: string) => {
    const newLocation: SavedLocation = {
      id: crypto.randomUUID(),
      name,
      x: panOffset().x,
      y: panOffset().y,
      zoom: zoom(),
      createdAt: new Date(),
    };
    const updated = [...savedLocations(), newLocation];
    setSavedLocations(updated);
    localStorage.setItem("plan-space-locations", JSON.stringify(updated));
  };

  const deleteLocation = (id: string) => {
    const updated = savedLocations().filter((l) => l.id !== id);
    setSavedLocations(updated);
    localStorage.setItem("plan-space-locations", JSON.stringify(updated));
  };

  const jumpToLocation = (location: SavedLocation) => {
    setPanOffset({ x: location.x, y: location.y });
    setZoom(location.zoom);
  };

  const goHome = () => {
    setPanOffset({ x: 0, y: 0 });
    setZoom(1);
  };

  const createGoal = async (goalData: Partial<Goal>) => {
    try {
      // Find a good position for new goal
      let x = 100 + Math.random() * 200;
      let y = 100 + Math.random() * 200;

      // Offset from pan
      x -= panOffset().x;
      y -= panOffset().y;

      const newGoal: Omit<Goal, "suid" | "createdAt" | "lastTouched"> = {
        title: goalData.title || "New Goal",
        description: goalData.description || "",
        energyRequired: goalData.energyRequired || "medium",
        meaningScore: goalData.meaningScore || 5,
        excitement: goalData.excitement || 5,
        progressType: "milestones",
        progress: 0,
        milestones: goalData.milestones || [],
        position: { x, y },
        relatedGoals: [],
        tags: goalData.tags || [],
      };

      const suid = await invoke<string>("plan_create_goal", { goal: newGoal });

      // Add to local state
      setGoals([
        ...goals(),
        {
          ...newGoal,
          suid,
          createdAt: new Date(),
          lastTouched: new Date(),
        },
      ]);

      setShowEditor(false);
    } catch (e) {
      console.error("Failed to create goal:", e);
    }
  };

  const updateGoal = async (goalData: Partial<Goal>) => {
    const goal = editingGoal();
    if (!goal) return;

    try {
      // Calculate progress from milestones if applicable
      let progress = goal.progress;
      if (goalData.milestones && goalData.milestones.length > 0) {
        const completed = goalData.milestones.filter((m) => m.completed).length;
        progress = Math.round((completed / goalData.milestones.length) * 100);
      }

      // Merge existing goal data with updates - backend expects full PlanGoal
      const fullUpdate = {
        title: goalData.title || goal.title,
        description: goalData.description || goal.description,
        energyRequired: goalData.energyRequired || goal.energyRequired,
        meaningScore: goalData.meaningScore ?? goal.meaningScore,
        excitement: goalData.excitement ?? goal.excitement,
        progressType: goal.progressType,
        progress,
        milestones: goalData.milestones || goal.milestones || [],
        position: goal.position,
        color: goal.color,
        relatedGoals: goal.relatedGoals,
        blockedBy: goal.blockedBy,
        tags: goalData.tags || goal.tags,
      };

      console.log("Updating goal with milestones:", fullUpdate.milestones);
      await invoke("plan_update_goal", { suid: goal.suid, updates: fullUpdate });

      // Update local state with full update
      setGoals(
        goals().map((g) =>
          g.suid === goal.suid ? { ...g, ...fullUpdate, lastTouched: new Date() } : g
        )
      );

      setShowEditor(false);
      setEditingGoal(null);
    } catch (e) {
      console.error("Failed to update goal:", e);
    }
  };

  // Update goal milestones directly (for step card interactions)
  const updateGoalMilestones = async (goalSuid: string, milestones: Milestone[]) => {
    const goal = goals().find((g) => g.suid === goalSuid);
    if (!goal) return;

    try {
      const completed = milestones.filter((m) => m.completed).length;
      const progress = milestones.length > 0
        ? Math.round((completed / milestones.length) * 100)
        : goal.progress;

      const fullUpdate = {
        title: goal.title,
        description: goal.description,
        energyRequired: goal.energyRequired,
        meaningScore: goal.meaningScore,
        excitement: goal.excitement,
        progressType: goal.progressType,
        progress,
        milestones,
        position: goal.position,
        color: goal.color,
        relatedGoals: goal.relatedGoals,
        blockedBy: goal.blockedBy,
        tags: goal.tags,
      };

      await invoke("plan_update_goal", { suid: goalSuid, updates: fullUpdate });

      setGoals(
        goals().map((g) =>
          g.suid === goalSuid ? { ...g, milestones, progress, lastTouched: new Date() } : g
        )
      );
    } catch (e) {
      console.error("Failed to update goal milestones:", e);
    }
  };

  // Toggle subtask completion
  const handleToggleSubtask = (goalSuid: string, milestoneId: string, subtaskId: string) => {
    const goal = goals().find((g) => g.suid === goalSuid);
    if (!goal || !goal.milestones) return;

    const updatedMilestones = goal.milestones.map((m) => {
      if (m.id === milestoneId && m.subtasks) {
        return {
          ...m,
          subtasks: m.subtasks.map((s) =>
            s.id === subtaskId ? { ...s, completed: !s.completed } : s
          ),
        };
      }
      return m;
    });

    updateGoalMilestones(goalSuid, updatedMilestones);
  };

  // Update a milestone (for toggling completion from step card)
  const handleUpdateMilestone = (goalSuid: string, updatedMilestone: Milestone) => {
    const goal = goals().find((g) => g.suid === goalSuid);
    if (!goal || !goal.milestones) return;

    const updatedMilestones = goal.milestones.map((m) =>
      m.id === updatedMilestone.id ? updatedMilestone : m
    );

    updateGoalMilestones(goalSuid, updatedMilestones);
  };

  // Launch an agent for a milestone
  const handleLaunchAgent = (goalSuid: string, milestoneId: string, agentType: AgentType) => {
    const goal = goals().find((g) => g.suid === goalSuid);
    if (!goal || !goal.milestones) return;

    const milestone = goal.milestones.find((m) => m.id === milestoneId);
    if (!milestone) return;

    // Generate markdown context for the agent
    const context = exportGoalToMarkdown(goal, milestone);

    // Call the parent's agent launcher if provided
    if (props.onLaunchAgent) {
      props.onLaunchAgent(agentType, context, milestone, goal);
    } else {
      // Default behavior: copy context to clipboard and log
      navigator.clipboard.writeText(context).then(() => {
        console.log(`Agent ${agentType} launched for milestone: ${milestone.title}`);
        console.log("Context copied to clipboard:\n", context);
      });
    }
  };

  // Step card drag handling
  const handleStepDragStart = (goalSuid: string, e: MouseEvent, milestoneId: string) => {
    const goal = goals().find((g) => g.suid === goalSuid);
    if (!goal || !goal.milestones) return;

    const milestone = goal.milestones.find((m) => m.id === milestoneId);
    if (!milestone) return;

    // Get current position (from milestone or calculated)
    const milestoneIndex = goal.milestones.findIndex((m) => m.id === milestoneId);
    const positions = calculateStepPositions(goal.position, goal.milestones.length);
    const currentPos = milestone.position || positions[milestoneIndex];

    setDraggingStep({ goalSuid, milestoneId });
    setStepDragOffset({
      x: e.clientX - currentPos.x * zoom() - panOffset().x,
      y: e.clientY - currentPos.y * zoom() - panOffset().y,
    });
  };

  const deleteGoal = async (suid: string) => {
    try {
      await invoke("plan_delete_goal", { suid });
      setGoals(goals().filter((g) => g.suid !== suid));
      setSelectedGoal(null);
      // Also remove from expanded goals
      const expanded = new Set(expandedGoals());
      expanded.delete(suid);
      setExpandedGoals(expanded);
    } catch (e) {
      console.error("Failed to delete goal:", e);
    }
  };

  // Widget CRUD operations
  const createWidget = async (widgetType: WidgetType) => {
    try {
      // Find a good position for new widget
      let x = 150 + Math.random() * 200;
      let y = 150 + Math.random() * 200;

      // Offset from pan
      x -= panOffset().x;
      y -= panOffset().y;

      const defaultData = {
        note: { type: "note" as const, content: "", color: "#fef3c7" },
        weather: { type: "weather" as const, location: "", units: "celsius" as const },
        calendar: { type: "calendar" as const, showWeekends: true },
        file: { type: "file" as const, filePath: "", isImage: false },
        link: { type: "link" as const, url: "", title: "" },
        clock: { type: "clock" as const, clockType: "digital", showSeconds: true, use24Hour: true },
      };

      const defaultSizes = {
        note: { width: 200, height: 200 },
        weather: { width: 240, height: 320 },
        calendar: { width: 320, height: 380 },
        file: { width: 180, height: 180 },
        clock: { width: 220, height: 200 },
        link: { width: 160, height: 80 },
      };

      const newWidget = {
        id: "",  // Backend will generate
        widgetType,
        title: getDefaultTitle(widgetType),
        position: { x, y },
        size: defaultSizes[widgetType],
        data: defaultData[widgetType],
        createdAt: new Date(),
        lastTouched: new Date(),
      };

      const id = await invoke<string>("plan_create_widget", { widget: newWidget });

      // Add to local state
      setWidgets([
        ...widgets(),
        {
          ...newWidget,
          id,
          type: "widget" as const,
        },
      ]);
    } catch (e) {
      console.error("Failed to create widget:", e);
    }
  };

  const getDefaultTitle = (widgetType: WidgetType): string => {
    const titles = {
      note: "New Note",
      weather: "Weather",
      calendar: "Calendar",
      file: "File",
      clock: "Clock",
      link: "Link",
    };
    return titles[widgetType];
  };

  const updateWidget = async (widget: Widget) => {
    try {
      const updateData = {
        id: widget.id,
        widgetType: widget.widgetType,
        title: widget.title,
        position: widget.position,
        size: widget.size,
        data: widget.data,
        createdAt: widget.createdAt,
        lastTouched: widget.lastTouched,
      };

      await invoke("plan_update_widget", { id: widget.id, updates: updateData });

      // Update local state - modify in-place to preserve component identity
      setWidgets((prevWidgets) => {
        const existing = prevWidgets.find((w) => w.id === widget.id);
        if (existing) {
          // Update in-place to prevent component remount
          Object.assign(existing, widget);
          return prevWidgets;
        }
        return [...prevWidgets, widget];
      });
    } catch (e) {
      console.error("Failed to update widget:", e);
    }
  };

  const deleteWidget = async (id: string) => {
    try {
      await invoke("plan_delete_widget", { id });
      setWidgets(widgets().filter((w) => w.id !== id));
      setSelectedWidget(null);
    } catch (e) {
      console.error("Failed to delete widget:", e);
    }
  };

  const saveWidgetPositions = async () => {
    try {
      const positions = widgets().map((w) => ({
        id: w.id,
        x: w.position.x,
        y: w.position.y,
        width: w.size?.width,
        height: w.size?.height,
      }));
      await invoke("plan_save_canvas_widgets", { positions });
    } catch (e) {
      console.debug("Failed to save widget positions:", e);
    }
  };

  const saveCanvasPositions = async () => {
    try {
      const positions = goals().map((g) => ({
        suid: g.suid,
        x: g.position.x,
        y: g.position.y,
      }));
      await invoke("plan_save_canvas", { positions });
    } catch (e) {
      console.debug("Failed to save canvas positions:", e);
    }
  };

  // Toggle expand/collapse for a goal
  const toggleGoalExpand = (suid: string) => {
    const expanded = new Set(expandedGoals());
    if (expanded.has(suid)) {
      expanded.delete(suid);
    } else {
      expanded.add(suid);
    }
    setExpandedGoals(expanded);
  };

  // Drag handling for goals
  const handleDragStart = (e: MouseEvent, suid: string) => {
    const goal = goals().find((g) => g.suid === suid);
    if (!goal) return;

    setDraggingGoal(suid);
    setDragOffset({
      x: e.clientX - goal.position.x * zoom() - panOffset().x,
      y: e.clientY - goal.position.y * zoom() - panOffset().y,
    });

    // Add dragging class
    const card = (e.target as HTMLElement).closest(".goal-card");
    card?.classList.add("dragging");
  };

  // Drag handling for widgets
  const handleWidgetDragStart = (e: MouseEvent, id: string) => {
    const widget = widgets().find((w) => w.id === id);
    if (!widget) return;

    setDraggingWidget(id);
    setDragOffset({
      x: e.clientX - widget.position.x * zoom() - panOffset().x,
      y: e.clientY - widget.position.y * zoom() - panOffset().y,
    });

    // Add dragging class
    const card = (e.target as HTMLElement).closest(".widget-card");
    card?.classList.add("dragging");
  };

  // Resize handling for widgets
  const handleWidgetResizeStart = (e: MouseEvent, id: string) => {
    const widget = widgets().find((w) => w.id === id);
    if (!widget || !widget.size) return;

    setResizingWidget(id);
    setResizeStart({
      x: e.clientX,
      y: e.clientY,
      width: widget.size.width,
      height: widget.size.height,
    });
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (draggingGoal()) {
      const newX = (e.clientX - dragOffset().x - panOffset().x) / zoom();
      const newY = (e.clientY - dragOffset().y - panOffset().y) / zoom();

      setGoals(
        goals().map((g) =>
          g.suid === draggingGoal()
            ? { ...g, position: { x: newX, y: newY } }
            : g
        )
      );
    } else if (draggingWidget()) {
      const newX = (e.clientX - dragOffset().x - panOffset().x) / zoom();
      const newY = (e.clientY - dragOffset().y - panOffset().y) / zoom();

      setWidgets(
        widgets().map((w) =>
          w.id === draggingWidget()
            ? { ...w, position: { x: newX, y: newY } }
            : w
        )
      );
    } else if (resizingWidget()) {
      const deltaX = e.clientX - resizeStart().x;
      const deltaY = e.clientY - resizeStart().y;

      setWidgets(
        widgets().map((w) => {
          if (w.id === resizingWidget() && w.size) {
            const newWidth = Math.max(120, resizeStart().width + deltaX / zoom());
            const newHeight = Math.max(120, resizeStart().height + deltaY / zoom());
            return {
              ...w,
              size: { width: newWidth, height: newHeight },
            };
          }
          return w;
        })
      );
    } else if (draggingStep()) {
      // Handle step card dragging
      const { goalSuid, milestoneId } = draggingStep()!;
      const newX = (e.clientX - stepDragOffset().x - panOffset().x) / zoom();
      const newY = (e.clientY - stepDragOffset().y - panOffset().y) / zoom();

      setGoals(
        goals().map((g) => {
          if (g.suid === goalSuid && g.milestones) {
            return {
              ...g,
              milestones: g.milestones.map((m) =>
                m.id === milestoneId
                  ? { ...m, position: { x: newX, y: newY } }
                  : m
              ),
            };
          }
          return g;
        })
      );
    } else if (isPanning()) {
      setPanOffset({
        x: e.clientX - panStart().x,
        y: e.clientY - panStart().y,
      });
    }
  };

  const handleMouseUp = () => {
    if (draggingGoal()) {
      // Remove dragging class
      document.querySelectorAll(".goal-card.dragging").forEach((el) => {
        el.classList.remove("dragging");
      });
      saveCanvasPositions();
    }
    if (draggingWidget()) {
      // Remove dragging class
      document.querySelectorAll(".widget-card.dragging").forEach((el) => {
        el.classList.remove("dragging");
      });
      saveWidgetPositions();
    }
    if (resizingWidget()) {
      // Save widget size
      saveWidgetPositions();
    }
    if (draggingStep()) {
      // Save milestone positions by updating the goal
      const { goalSuid } = draggingStep()!;
      const goal = goals().find((g) => g.suid === goalSuid);
      if (goal && goal.milestones) {
        updateGoalMilestones(goalSuid, goal.milestones);
      }
    }
    setDraggingGoal(null);
    setDraggingWidget(null);
    setResizingWidget(null);
    setDraggingStep(null);
    setIsPanning(false);
  };

  // Canvas panning
  const handleCanvasMouseDown = (e: MouseEvent) => {
    // Only pan on left click on empty canvas area
    if (e.target === e.currentTarget || (e.target as HTMLElement).classList.contains("plan-canvas")) {
      setIsPanning(true);
      setPanStart({
        x: e.clientX - panOffset().x,
        y: e.clientY - panOffset().y,
      });
      setSelectedGoal(null);
    }
  };

  // Zoom handling
  const handleWheel = (e: WheelEvent) => {
    e.preventDefault();

    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    const delta = e.deltaY > 0 ? -0.1 : 0.1;
    const oldZoom = zoom();
    const newZoom = Math.max(0.25, Math.min(2, oldZoom + delta));

    // Calculate the point in world coordinates under the cursor before zoom
    const worldX = (mouseX - panOffset().x) / oldZoom;
    const worldY = (mouseY - panOffset().y) / oldZoom;

    // Calculate new pan offset to keep the same world point under the cursor
    const newPanX = mouseX - worldX * newZoom;
    const newPanY = mouseY - worldY * newZoom;

    setZoom(newZoom);
    setPanOffset({ x: newPanX, y: newPanY });
  };

  const zoomIn = () => setZoom(Math.min(2, zoom() + 0.25));
  const zoomOut = () => setZoom(Math.max(0.25, zoom() - 0.25));
  const resetZoom = () => {
    setZoom(1);
    setPanOffset({ x: 0, y: 0 });
  };

  // Open editor
  const openNewGoalEditor = () => {
    setEditingGoal(null);
    setIsNewGoal(true);
    setShowEditor(true);
  };

  const openEditGoalEditor = (suid: string) => {
    const goal = goals().find((g) => g.suid === suid);
    if (goal) {
      setEditingGoal(goal);
      setIsNewGoal(false);
      setShowEditor(true);
    }
  };

  return (
    <div class="plan-space">
      {/* Header */}
      <header class="plan-space-header">
        <div class="plan-space-header-left">
          <h1 class="plan-space-title">Plan Space</h1>
          <div class="energy-selector">
            <span class="energy-selector-label">Energy:</span>
            <div class="energy-dots">
              {[1, 2, 3, 4, 5].map((level) => (
                <div
                  class={`energy-dot ${currentEnergy() >= level ? "active" : ""}`}
                  onClick={() => setCurrentEnergy(level)}
                />
              ))}
            </div>
          </div>
        </div>
        <div class="plan-space-header-right">
          <LocationPicker
            locations={savedLocations()}
            onSaveLocation={saveCurrentLocation}
            onJumpTo={jumpToLocation}
            onDelete={deleteLocation}
            onGoHome={goHome}
          />
          <button class="plan-btn plan-btn-secondary" onClick={() => setShowWidgetPicker(true)}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <rect x="1" y="2" width="11" height="10" rx="1" stroke="currentColor" stroke-width="1.5" />
              <path d="M5 1v3M8 1v3" stroke="currentColor" stroke-width="1.5" />
            </svg>
            Add Widget
          </button>
          <button class="plan-btn plan-btn-primary" onClick={openNewGoalEditor}>
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path d="M7 1v12M1 7h12" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
            </svg>
            New Goal
          </button>
        </div>
      </header>

      {/* Close button */}
      <button class="plan-close-btn" onClick={props.onClose} title="Close (Esc)">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>

      {/* Canvas */}
      <div
        class="plan-canvas-container"
        onMouseDown={handleCanvasMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onWheel={handleWheel}
      >
        <div
          class="plan-canvas"
          style={{
            transform: `translate(${panOffset().x}px, ${panOffset().y}px) scale(${zoom()})`,
          }}
        >
          <Show when={!loading() && goals().length === 0}>
            <div class="plan-empty">
              <div class="plan-empty-icon">{"🎯"}</div>
              <h2 class="plan-empty-title">Your canvas awaits</h2>
              <p class="plan-empty-text">
                Create your first goal and arrange your ideas spatially.
                Goals float in space - proximity means relationship.
              </p>
              <button class="plan-btn plan-btn-primary" onClick={openNewGoalEditor}>
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                  <path d="M7 1v12M1 7h12" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                </svg>
                Create First Goal
              </button>
            </div>
          </Show>

          {/* Connection lines between goals and step cards */}
          <svg class="plan-connections" style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "100%", "pointer-events": "none", overflow: "visible" }}>
            <For each={goals()}>
              {(goal) => (
                <Show when={expandedGoals().has(goal.suid) && goal.milestones && goal.milestones.length > 0}>
                  <For each={goal.milestones}>
                    {(milestone, index) => {
                      const defaultPositions = calculateStepPositions(goal.position, goal.milestones!.length);
                      const stepPos = milestone.position || defaultPositions[index()];
                      // Goal card center (approx 240px wide, 150px tall)
                      const goalCenterX = goal.position.x + 120;
                      const goalCenterY = goal.position.y + 75;
                      // Step card center (180px wide, ~120px tall)
                      const stepCenterX = stepPos.x + 90;
                      const stepCenterY = stepPos.y + 60;
                      return (
                        <line
                          x1={goalCenterX}
                          y1={goalCenterY}
                          x2={stepCenterX}
                          y2={stepCenterY}
                          stroke="var(--plan-accent)"
                          stroke-width="2"
                          stroke-dasharray="6,4"
                          opacity="0.6"
                        />
                      );
                    }}
                  </For>
                </Show>
              )}
            </For>
          </svg>

          {/* Goal Cards */}
          <For each={goals()}>
            {(goal) => (
              <GoalCard
                goal={goal}
                selected={selectedGoal() === goal.suid}
                isExpanded={expandedGoals().has(goal.suid)}
                onSelect={setSelectedGoal}
                onDragStart={handleDragStart}
                onDoubleClick={openEditGoalEditor}
                onToggleExpand={toggleGoalExpand}
              />
            )}
          </For>

          {/* Widget Cards */}
          <For each={widgets()}>
            {(widget) => (
              <WidgetCard
                widget={widget}
                selected={selectedWidget() === widget.id}
                onSelect={setSelectedWidget}
                onDragStart={handleWidgetDragStart}
                onDoubleClick={() => {}}
                onUpdate={updateWidget}
                onDelete={deleteWidget}
                onResizeStart={handleWidgetResizeStart}
              />
            )}
          </For>

          {/* Step Cards for expanded goals */}
          <For each={goals()}>
            {(goal) => (
              <Show when={expandedGoals().has(goal.suid) && goal.milestones && goal.milestones.length > 0}>
                <For each={goal.milestones}>
                  {(milestone, index) => {
                    // Use saved position or calculate default
                    const defaultPositions = calculateStepPositions(goal.position, goal.milestones!.length);
                    const position = milestone.position || defaultPositions[index()];
                    return (
                      <StepCard
                        milestone={milestone}
                        parentGoalSuid={goal.suid}
                        position={position}
                        onToggleSubtask={(milestoneId, subtaskId) =>
                          handleToggleSubtask(goal.suid, milestoneId, subtaskId)
                        }
                        onUpdateMilestone={(m) =>
                          handleUpdateMilestone(goal.suid, m)
                        }
                        onLaunchAgent={(milestoneId, agentType) =>
                          handleLaunchAgent(goal.suid, milestoneId, agentType)
                        }
                        onDragStart={(e, milestoneId) =>
                          handleStepDragStart(goal.suid, e, milestoneId)
                        }
                      />
                    );
                  }}
                </For>
              </Show>
            )}
          </For>
        </div>
      </div>

      {/* Zoom controls */}
      <div class="plan-zoom-controls">
        <button class="plan-zoom-btn" onClick={zoomIn} title="Zoom in">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 3v8M3 7h8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
        <div class="plan-zoom-level">{Math.round(zoom() * 100)}%</div>
        <button class="plan-zoom-btn" onClick={zoomOut} title="Zoom out">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M3 7h8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
        <button class="plan-zoom-btn" onClick={goHome} title="Home (reset view)">
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1L2 5v6h2V7h6v4h2V5L7 1z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>
          </svg>
        </button>
      </div>

      {/* Whisper bar placeholder */}
      <div class="plan-whisper-bar">
        <div class="plan-whisper-icon">AI</div>
        <span class="plan-whisper-text">
          {goals().length === 0 && widgets().length === 0
            ? "Create a goal or add a widget to get started. I'll help you find connections and patterns."
            : `${goals().length} goal${goals().length !== 1 ? "s" : ""}, ${widgets().length} widget${widgets().length !== 1 ? "s" : ""} on your canvas`}
        </span>
      </div>

      {/* Goal Editor */}
      <Show when={showEditor()}>
        <GoalEditor
          goal={editingGoal()}
          isNew={isNewGoal()}
          onSave={isNewGoal() ? createGoal : updateGoal}
          onDelete={editingGoal() ? () => deleteGoal(editingGoal()!.suid) : undefined}
          onClose={() => {
            setShowEditor(false);
            setEditingGoal(null);
          }}
        />
      </Show>

      {/* Widget Picker */}
      <WidgetPicker
        show={showWidgetPicker()}
        onClose={() => setShowWidgetPicker(false)}
        onSelectWidgetType={createWidget}
      />
    </div>
  );
};

export default PlanSpace;
