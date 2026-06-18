import { Component, createSignal, Show, For } from "solid-js";
import { EditorView } from "@codemirror/view";
import {
  Annotation,
  createAnnotation,
  getAnnotations,
  removeAnnotation,
  resolveAnnotation,
} from "./annotation-extension";
import "./AnnotationPanel.css";

interface AnnotationPanelProps {
  editorView?: EditorView;
  onClose: () => void;
}

type AnnotationType = Annotation["type"];

const ANNOTATION_TYPES: { value: AnnotationType; label: string; color: string }[] = [
  { value: "comment", label: "Comment", color: "#569cd6" },
  { value: "todo", label: "TODO", color: "#dcdcaa" },
  { value: "citation-needed", label: "Citation Needed", color: "#ce9178" },
  { value: "highlight", label: "Highlight", color: "#4ec9b0" },
];

const AnnotationPanel: Component<AnnotationPanelProps> = (props) => {
  const [newText, setNewText] = createSignal("");
  const [newType, setNewType] = createSignal<AnnotationType>("comment");
  const [annotations, setAnnotations] = createSignal<Annotation[]>([]);
  const [showResolved, setShowResolved] = createSignal(false);

  const refreshAnnotations = () => {
    if (props.editorView) {
      setAnnotations(getAnnotations(props.editorView));
    }
  };

  // Refresh on any state change
  const checkForUpdates = () => {
    refreshAnnotations();
    requestAnimationFrame(checkForUpdates);
  };
  requestAnimationFrame(checkForUpdates);

  const handleAdd = () => {
    if (!props.editorView || !newText().trim()) return;
    const result = createAnnotation(props.editorView, newType(), newText().trim());
    if (result) {
      setNewText("");
      refreshAnnotations();
    }
  };

  const handleRemove = (id: string) => {
    if (!props.editorView) return;
    props.editorView.dispatch({
      effects: removeAnnotation.of(id),
    });
    refreshAnnotations();
  };

  const handleResolve = (id: string) => {
    if (!props.editorView) return;
    props.editorView.dispatch({
      effects: resolveAnnotation.of(id),
    });
    refreshAnnotations();
  };

  const scrollToAnnotation = (a: Annotation) => {
    if (!props.editorView) return;
    props.editorView.dispatch({
      selection: { anchor: a.from, head: a.to },
      effects: EditorView.scrollIntoView(a.from, { y: "center" }),
    });
    props.editorView.focus();
  };

  const filteredAnnotations = () => {
    const all = annotations();
    return showResolved() ? all : all.filter((a) => !a.resolved);
  };

  const resolvedCount = () => annotations().filter((a) => a.resolved).length;

  return (
    <div class="ann-panel">
      <div class="ann-header">
        <span class="ann-title">Annotations</span>
        <button class="ann-close" onClick={props.onClose}>x</button>
      </div>

      <div class="ann-add">
        <div class="ann-add-row">
          <select
            class="ann-type-select"
            value={newType()}
            onChange={(e) => setNewType(e.currentTarget.value as AnnotationType)}
          >
            <For each={ANNOTATION_TYPES}>
              {(t) => <option value={t.value}>{t.label}</option>}
            </For>
          </select>
        </div>
        <div class="ann-add-row">
          <input
            class="ann-text-input"
            type="text"
            placeholder="Select text, then add note..."
            value={newText()}
            onInput={(e) => setNewText(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          />
          <button class="ann-add-btn" onClick={handleAdd}>Add</button>
        </div>
      </div>

      <div class="ann-filter">
        <span>{filteredAnnotations().length} annotation{filteredAnnotations().length !== 1 ? "s" : ""}</span>
        <Show when={resolvedCount() > 0}>
          <button
            class="ann-toggle-resolved"
            onClick={() => setShowResolved(!showResolved())}
          >
            {showResolved() ? "Hide" : "Show"} resolved ({resolvedCount()})
          </button>
        </Show>
      </div>

      <div class="ann-list">
        <For each={filteredAnnotations()}>
          {(a) => (
            <div
              class="ann-item"
              classList={{ "ann-item-resolved": a.resolved }}
              onClick={() => scrollToAnnotation(a)}
            >
              <div class="ann-item-header">
                <span class={`ann-type-badge ann-badge-${a.type}`}>
                  {ANNOTATION_TYPES.find((t) => t.value === a.type)?.label || a.type}
                </span>
                <div class="ann-item-actions">
                  <Show when={!a.resolved}>
                    <button
                      class="ann-action"
                      title="Resolve"
                      onClick={(e) => { e.stopPropagation(); handleResolve(a.id); }}
                    >
                      &#10003;
                    </button>
                  </Show>
                  <button
                    class="ann-action ann-action-delete"
                    title="Delete"
                    onClick={(e) => { e.stopPropagation(); handleRemove(a.id); }}
                  >
                    &#10005;
                  </button>
                </div>
              </div>
              <div class="ann-item-text">{a.text}</div>
            </div>
          )}
        </For>
      </div>
    </div>
  );
};

export default AnnotationPanel;
