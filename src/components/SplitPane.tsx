import { Component, JSX, createSignal, onMount, onCleanup } from "solid-js";

interface SplitPaneProps {
  left: JSX.Element;
  right: JSX.Element;
  storageKey?: string;
  defaultRatio?: number;
  minLeftPx?: number;
  minRightPx?: number;
}

const SplitPane: Component<SplitPaneProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  const storageKey = props.storageKey || "splitpane-ratio";
  const defaultRatio = props.defaultRatio ?? 0.5;
  const minLeft = props.minLeftPx ?? 200;
  const minRight = props.minRightPx ?? 200;

  const saved = localStorage.getItem(storageKey);
  const [ratio, setRatio] = createSignal(saved ? parseFloat(saved) : defaultRatio);
  const [dragging, setDragging] = createSignal(false);

  const onPointerDown = (e: PointerEvent) => {
    e.preventDefault();
    setDragging(true);
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: PointerEvent) => {
    if (!dragging() || !containerRef) return;
    const rect = containerRef.getBoundingClientRect();
    const totalWidth = rect.width;
    const x = e.clientX - rect.left;

    // Clamp to min sizes
    const clampedX = Math.max(minLeft, Math.min(totalWidth - minRight, x));
    const newRatio = clampedX / totalWidth;
    setRatio(newRatio);
  };

  const onPointerUp = () => {
    if (dragging()) {
      setDragging(false);
      localStorage.setItem(storageKey, ratio().toString());
    }
  };

  onMount(() => {
    document.addEventListener("pointerup", onPointerUp);
  });

  onCleanup(() => {
    document.removeEventListener("pointerup", onPointerUp);
  });

  return (
    <div
      ref={containerRef}
      class="split-pane-container"
      classList={{ "split-pane-dragging": dragging() }}
      onPointerMove={onPointerMove}
    >
      <div class="split-pane-left" style={{ width: `${ratio() * 100}%` }}>
        {props.left}
      </div>
      <div
        class="split-pane-divider"
        onPointerDown={onPointerDown}
      />
      <div class="split-pane-right" style={{ width: `${(1 - ratio()) * 100}%` }}>
        {props.right}
      </div>
    </div>
  );
};

export default SplitPane;
