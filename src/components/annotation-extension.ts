import {
  StateField,
  StateEffect,
  RangeSet,
  Extension,
} from "@codemirror/state";
import {
  EditorView,
  Decoration,
  GutterMarker,
  gutter,
} from "@codemirror/view";

// --- Types ---

export interface Annotation {
  id: string;
  from: number;
  to: number;
  text: string;
  type: "comment" | "todo" | "citation-needed" | "highlight";
  resolved: boolean;
  createdAt: string;
}

// --- Effects ---

export const addAnnotation = StateEffect.define<Annotation>();
export const removeAnnotation = StateEffect.define<string>(); // by id
export const resolveAnnotation = StateEffect.define<string>(); // by id

// --- Gutter Marker ---

class AnnotationMarkerWidget extends GutterMarker {
  constructor(
    readonly count: number,
    readonly type: string
  ) {
    super();
  }

  toDOM() {
    const el = document.createElement("span");
    el.className = `annotation-gutter-marker annotation-type-${this.type}`;
    el.textContent = this.count > 1 ? `${this.count}` : "\u25CF"; // bullet
    el.title = `${this.count} annotation${this.count > 1 ? "s" : ""}`;
    return el;
  }
}

// --- State Field ---

export const annotationField = StateField.define<Annotation[]>({
  create() {
    return [];
  },
  update(annotations, tr) {
    let updated = [...annotations];

    // Adjust positions for document changes
    if (tr.docChanged) {
      updated = updated.map((a) => {
        const newFrom = tr.changes.mapPos(a.from, 1);
        const newTo = tr.changes.mapPos(a.to, -1);
        if (newFrom >= newTo) return null; // annotation collapsed
        return { ...a, from: newFrom, to: newTo };
      }).filter((a): a is Annotation => a !== null);
    }

    for (const effect of tr.effects) {
      if (effect.is(addAnnotation)) {
        updated = [...updated, effect.value];
      } else if (effect.is(removeAnnotation)) {
        updated = updated.filter((a) => a.id !== effect.value);
      } else if (effect.is(resolveAnnotation)) {
        updated = updated.map((a) =>
          a.id === effect.value ? { ...a, resolved: true } : a
        );
      }
    }

    return updated;
  },
});

// --- Decorations ---

const highlightMark = Decoration.mark({ class: "cm-annotation-highlight" });
const commentMark = Decoration.mark({ class: "cm-annotation-comment" });
const todoMark = Decoration.mark({ class: "cm-annotation-todo" });
const citationMark = Decoration.mark({ class: "cm-annotation-citation" });

function getMarkForType(type: string) {
  switch (type) {
    case "highlight": return highlightMark;
    case "todo": return todoMark;
    case "citation-needed": return citationMark;
    default: return commentMark;
  }
}

const annotationDecorations = EditorView.decorations.compute(
  [annotationField],
  (state) => {
    const annotations = state.field(annotationField);
    const marks = annotations
      .filter((a) => !a.resolved && a.from < a.to && a.to <= state.doc.length)
      .map((a) => getMarkForType(a.type).range(a.from, a.to))
      .sort((a, b) => a.from - b.from);
    return Decoration.set(marks);
  }
);

// --- Gutter ---

const annotationGutter = gutter({
  class: "cm-annotation-gutter",
  markers(view) {
    const annotations = view.state.field(annotationField);
    const lineMarkers: Map<number, { count: number; type: string }> = new Map();

    for (const a of annotations) {
      if (a.resolved) continue;
      const line = view.state.doc.lineAt(a.from).number;
      const existing = lineMarkers.get(line);
      if (existing) {
        existing.count++;
      } else {
        lineMarkers.set(line, { count: 1, type: a.type });
      }
    }

    const markers: { from: number; marker: GutterMarker }[] = [];
    for (const [lineNum, info] of lineMarkers) {
      const line = view.state.doc.line(lineNum);
      markers.push({
        from: line.from,
        marker: new AnnotationMarkerWidget(info.count, info.type),
      });
    }

    return RangeSet.of(
      markers.sort((a, b) => a.from - b.from).map((m) => m.marker.range(m.from))
    );
  },
});

// --- Public API ---

export function createAnnotation(
  view: EditorView,
  type: Annotation["type"],
  text: string
): Annotation | null {
  const { from, to } = view.state.selection.main;
  if (from === to) return null; // need a selection

  const annotation: Annotation = {
    id: crypto.randomUUID(),
    from,
    to,
    text,
    type,
    resolved: false,
    createdAt: new Date().toISOString(),
  };

  view.dispatch({
    effects: addAnnotation.of(annotation),
  });

  return annotation;
}

export function getAnnotations(view: EditorView): Annotation[] {
  return view.state.field(annotationField);
}

// --- Extension ---

export function annotationExtension(): Extension {
  return [annotationField, annotationDecorations, annotationGutter];
}
