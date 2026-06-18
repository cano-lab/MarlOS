import { Component, createSignal, createEffect, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import "./ProjectManager.css";

interface ProjectView {
  id: string;
  name: string;
  description: string;
  status: "active" | "paused" | "completed" | "archived";
  paper_count: number;
  reference_count: number;
  document_count: number;
  tags: string[];
  deadline: string | null;
  created_at: string;
  modified_at: string;
}

interface ProjectManagerProps {
  onClose: () => void;
  onSelectProject?: (projectId: string) => void;
}

const ProjectManager: Component<ProjectManagerProps> = (props) => {
  const [projects, setProjects] = createSignal<ProjectView[]>([]);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [showCreate, setShowCreate] = createSignal(false);
  const [newName, setNewName] = createSignal("");
  const [newDesc, setNewDesc] = createSignal("");
  const [filter, setFilter] = createSignal<string>("all");

  const loadProjects = async () => {
    try {
      const list = await invoke<ProjectView[]>("project_list");
      setProjects(list);
    } catch (e) {
      console.error("Failed to load projects:", e);
    }
  };

  createEffect(() => { loadProjects(); });

  const createProject = async () => {
    if (!newName().trim()) return;
    try {
      await invoke("project_create", { name: newName(), description: newDesc() });
      setNewName("");
      setNewDesc("");
      setShowCreate(false);
      loadProjects();
    } catch (e) {
      console.error("Failed to create project:", e);
    }
  };

  const deleteProject = async (id: string) => {
    try {
      await invoke("project_delete", { projectId: id });
      if (selectedId() === id) setSelectedId(null);
      loadProjects();
    } catch (e) {
      console.error("Failed to delete project:", e);
    }
  };

  const setStatus = async (id: string, status: string) => {
    try {
      await invoke("project_set_status", { projectId: id, status });
      loadProjects();
    } catch (e) {
      console.error("Failed to update status:", e);
    }
  };

  const filtered = () => {
    const f = filter();
    if (f === "all") return projects();
    return projects().filter(p => p.status === f);
  };

  const statusIcon = (status: string) => {
    switch (status) {
      case "active": return "\u25B6"; // play
      case "paused": return "\u23F8"; // pause
      case "completed": return "\u2713"; // check
      case "archived": return "\u2610"; // box
      default: return "";
    }
  };

  const statusColor = (status: string) => {
    switch (status) {
      case "active": return "var(--accent, #569cd6)";
      case "paused": return "#dcdcaa";
      case "completed": return "#4ec9b0";
      case "archived": return "#858585";
      default: return "#d4d4d4";
    }
  };

  return (
    <div class="project-manager-overlay" onClick={(e) => { if (e.target === e.currentTarget) props.onClose(); }}>
      <div class="project-manager-panel">
        <div class="pm-header">
          <h2>Research Projects</h2>
          <div class="pm-header-actions">
            <button class="pm-btn pm-btn-primary" onClick={() => setShowCreate(true)}>+ New Project</button>
            <button class="pm-close" onClick={props.onClose}>&times;</button>
          </div>
        </div>

        <div class="pm-filters">
          <button class={`pm-filter ${filter() === "all" ? "active" : ""}`} onClick={() => setFilter("all")}>All</button>
          <button class={`pm-filter ${filter() === "active" ? "active" : ""}`} onClick={() => setFilter("active")}>Active</button>
          <button class={`pm-filter ${filter() === "paused" ? "active" : ""}`} onClick={() => setFilter("paused")}>Paused</button>
          <button class={`pm-filter ${filter() === "completed" ? "active" : ""}`} onClick={() => setFilter("completed")}>Completed</button>
          <button class={`pm-filter ${filter() === "archived" ? "active" : ""}`} onClick={() => setFilter("archived")}>Archived</button>
        </div>

        <Show when={showCreate()}>
          <div class="pm-create-form">
            <input
              type="text"
              placeholder="Project name..."
              value={newName()}
              onInput={(e) => setNewName(e.currentTarget.value)}
              onKeyDown={(e) => { if (e.key === "Enter") createProject(); }}
              autofocus
            />
            <textarea
              placeholder="Description (optional)..."
              value={newDesc()}
              onInput={(e) => setNewDesc(e.currentTarget.value)}
              rows={2}
            />
            <div class="pm-create-actions">
              <button class="pm-btn pm-btn-primary" onClick={createProject}>Create</button>
              <button class="pm-btn" onClick={() => setShowCreate(false)}>Cancel</button>
            </div>
          </div>
        </Show>

        <div class="pm-project-list">
          <For each={filtered()} fallback={<div class="pm-empty">No projects yet. Create one to get started.</div>}>
            {(project) => (
              <div
                class={`pm-project-card ${selectedId() === project.id ? "selected" : ""}`}
                onClick={() => {
                  setSelectedId(project.id);
                  props.onSelectProject?.(project.id);
                }}
              >
                <div class="pm-project-header">
                  <span class="pm-status-badge" style={{ color: statusColor(project.status) }}>
                    {statusIcon(project.status)}
                  </span>
                  <h3>{project.name}</h3>
                  <div class="pm-project-actions">
                    <select
                      value={project.status}
                      onChange={(e) => setStatus(project.id, e.currentTarget.value)}
                      onClick={(e) => e.stopPropagation()}
                    >
                      <option value="active">Active</option>
                      <option value="paused">Paused</option>
                      <option value="completed">Completed</option>
                      <option value="archived">Archived</option>
                    </select>
                    <button
                      class="pm-btn-icon pm-btn-danger"
                      title="Delete project"
                      onClick={(e) => { e.stopPropagation(); deleteProject(project.id); }}
                    >
                      &times;
                    </button>
                  </div>
                </div>
                <Show when={project.description}>
                  <p class="pm-project-desc">{project.description}</p>
                </Show>
                <div class="pm-project-stats">
                  <span title="Papers">{project.paper_count} papers</span>
                  <span title="References">{project.reference_count} refs</span>
                  <span title="Documents">{project.document_count} docs</span>
                </div>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
};

export default ProjectManager;
