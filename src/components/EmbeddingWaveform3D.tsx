import { Component, createSignal, createEffect, onMount, onCleanup, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import "./EmbeddingWaveform3D.css";

interface SearchResult {
  suid: string;
  name: string;
  score: number;
}

interface EmbeddingWaveform3DProps {
  isOpen: boolean;
  onClose: () => void;
}

const EmbeddingWaveform3D: Component<EmbeddingWaveform3DProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let renderer: THREE.WebGLRenderer | null = null;
  let scene: THREE.Scene | null = null;
  let camera: THREE.PerspectiveCamera | null = null;
  let controls: OrbitControls | null = null;
  let animationId: number | null = null;
  let waveformMesh: THREE.Object3D | null = null;
  let lineMesh: THREE.Line | null = null;

  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [selectedSuid, setSelectedSuid] = createSignal<string | null>(null);
  const [embedding, setEmbedding] = createSignal<number[] | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [displayMode, setDisplayMode] = createSignal<"ribbon" | "tube" | "surface" | "helix">("ribbon");
  const [colorMode, setColorMode] = createSignal<"gradient" | "value" | "position">("value");
  const [autoRotate, setAutoRotate] = createSignal(true);

  const initThree = () => {
    if (!containerRef) return;

    // Scene
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x1a1a2e);

    // Camera
    const width = containerRef.clientWidth;
    const height = containerRef.clientHeight;
    camera = new THREE.PerspectiveCamera(60, width / height, 0.1, 1000);
    camera.position.set(0, 5, 15);

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(width, height);
    renderer.setPixelRatio(window.devicePixelRatio);
    containerRef.appendChild(renderer.domElement);

    // Controls
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.autoRotate = autoRotate();
    controls.autoRotateSpeed = 0.5;

    // Lighting
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.5);
    scene.add(ambientLight);

    const directionalLight = new THREE.DirectionalLight(0xffffff, 1);
    directionalLight.position.set(5, 10, 5);
    scene.add(directionalLight);

    // Grid
    const gridHelper = new THREE.GridHelper(20, 20, 0x444444, 0x333333);
    scene.add(gridHelper);

    // Axes
    const axesHelper = new THREE.AxesHelper(5);
    scene.add(axesHelper);

    animate();
  };

  const animate = () => {
    if (!renderer || !scene || !camera || !controls) return;

    animationId = requestAnimationFrame(animate);
    controls.update();
    renderer.render(scene, camera);
  };

  const cleanup = () => {
    if (animationId) {
      cancelAnimationFrame(animationId);
    }
    if (renderer && containerRef) {
      containerRef.removeChild(renderer.domElement);
      renderer.dispose();
    }
    renderer = null;
    scene = null;
    camera = null;
    controls = null;
  };

  const createWaveformGeometry = (emb: number[]) => {
    if (!scene) return;

    // Remove existing mesh
    if (waveformMesh) {
      scene.remove(waveformMesh);
      waveformMesh = null;
    }
    if (lineMesh) {
      scene.remove(lineMesh);
      lineMesh = null;
    }

    // Normalize embedding
    let min = Infinity, max = -Infinity;
    for (const v of emb) {
      if (v < min) min = v;
      if (v > max) max = v;
    }
    const range = max - min || 1;

    const mode = displayMode();
    const len = emb.length;
    const colors: number[] = [];

    if (mode === "ribbon") {
      // Create a ribbon that flows through 3D space
      const geometry = new THREE.BufferGeometry();
      const vertices: number[] = [];

      for (let i = 0; i < len; i++) {
        const t = i / len;
        const x = (i / len) * 16 - 8; // -8 to 8
        const normalized = (emb[i] - min) / range;
        const y = normalized * 4 - 2; // -2 to 2

        // Create ribbon width
        vertices.push(x, y, -0.2);
        vertices.push(x, y, 0.2);

        // Color based on value
        const color = getColor(normalized, t);
        colors.push(color.r, color.g, color.b);
        colors.push(color.r, color.g, color.b);
      }

      // Create faces
      const indices: number[] = [];
      for (let i = 0; i < len - 1; i++) {
        const idx = i * 2;
        indices.push(idx, idx + 1, idx + 2);
        indices.push(idx + 1, idx + 3, idx + 2);
      }

      geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
      geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
      geometry.setIndex(indices);
      geometry.computeVertexNormals();

      const material = new THREE.MeshPhongMaterial({
        vertexColors: true,
        side: THREE.DoubleSide,
        shininess: 50,
      });

      waveformMesh = new THREE.Mesh(geometry, material);
      scene.add(waveformMesh);

    } else if (mode === "tube") {
      // Create a 3D tube following the waveform
      const points: THREE.Vector3[] = [];

      for (let i = 0; i < len; i++) {
        const x = (i / len) * 16 - 8;
        const normalized = (emb[i] - min) / range;
        const y = normalized * 4 - 2;
        const z = Math.sin((i / len) * Math.PI * 4) * 2; // Spiral through Z
        points.push(new THREE.Vector3(x, y, z));
      }

      const curve = new THREE.CatmullRomCurve3(points);
      const geometry = new THREE.TubeGeometry(curve, len, 0.1, 8, false);

      // Create gradient material
      const material = new THREE.MeshPhongMaterial({
        color: 0x4a9eff,
        shininess: 100,
      });

      waveformMesh = new THREE.Mesh(geometry, material);
      scene.add(waveformMesh);

    } else if (mode === "surface") {
      // Create a 2D surface plot (embedding as X, position as Y, value as Z)
      const segmentSize = Math.ceil(Math.sqrt(len));
      const geometry = new THREE.PlaneGeometry(16, 16, segmentSize - 1, segmentSize - 1);
      const positions = geometry.attributes.position.array as Float32Array;

      for (let i = 0; i < len && i < positions.length / 3; i++) {
        const normalized = (emb[i] - min) / range;
        positions[i * 3 + 2] = normalized * 4 - 2; // Z as height

        const color = getColor(normalized, i / len);
        colors.push(color.r, color.g, color.b);
      }

      geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
      geometry.computeVertexNormals();

      const material = new THREE.MeshPhongMaterial({
        vertexColors: true,
        side: THREE.DoubleSide,
        wireframe: false,
      });

      waveformMesh = new THREE.Mesh(geometry, material);
      waveformMesh.rotation.x = -Math.PI / 4;
      scene.add(waveformMesh);

    } else if (mode === "helix") {
      // Create a DNA-like helix representation
      const geometry = new THREE.BufferGeometry();
      const vertices: number[] = [];

      for (let i = 0; i < len; i++) {
        const t = i / len;
        const angle = t * Math.PI * 8; // 4 full rotations
        const radius = 3;
        const normalized = (emb[i] - min) / range;

        // Position on helix
        const x = Math.cos(angle) * radius;
        const y = (t - 0.5) * 16; // Height along Y
        const z = Math.sin(angle) * radius;

        // Offset by value
        const offset = normalized - 0.5;
        vertices.push(
          x + Math.cos(angle) * offset * 2,
          y,
          z + Math.sin(angle) * offset * 2
        );

        const color = getColor(normalized, t);
        colors.push(color.r, color.g, color.b);
      }

      geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
      geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));

      const material = new THREE.PointsMaterial({
        size: 0.15,
        vertexColors: true,
      });

      waveformMesh = new THREE.Points(geometry, material);
      scene.add(waveformMesh);

      // Add connecting line
      const lineMaterial = new THREE.LineBasicMaterial({
        vertexColors: true,
        opacity: 0.5,
        transparent: true,
      });
      lineMesh = new THREE.Line(geometry.clone(), lineMaterial);
      scene.add(lineMesh);
    }
  };

  const getColor = (value: number, position: number): THREE.Color => {
    const mode = colorMode();

    if (mode === "gradient") {
      // Blue to purple gradient based on position
      return new THREE.Color().setHSL(0.6 + position * 0.2, 0.8, 0.5);
    } else if (mode === "value") {
      // Color based on value (blue=low, green=mid, red=high)
      if (value < 0.33) {
        return new THREE.Color(0x4a9eff);
      } else if (value < 0.66) {
        return new THREE.Color(0x4caf50);
      } else {
        return new THREE.Color(0xff9800);
      }
    } else {
      // Position-based hue
      return new THREE.Color().setHSL(position, 0.8, 0.5);
    }
  };

  const handleSearch = async () => {
    if (!searchQuery().trim()) return;

    setLoading(true);
    try {
      const results = await invoke<SearchResult[]>("semantic_search", {
        query: searchQuery(),
        limit: 10,
      });
      setSearchResults(results);
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      setLoading(false);
    }
  };

  const selectObject = async (suid: string) => {
    console.log("selectObject called with suid:", suid);
    setSelectedSuid(suid);
    setLoading(true);

    try {
      // Use the dedicated embedding fetch command
      console.log("Invoking get_object_embedding...");
      const emb = await invoke<number[] | null>("get_object_embedding", { suid });
      console.log("Got embedding result:", emb ? `${emb.length} dimensions` : "null");

      if (emb && emb.length > 0) {
        setEmbedding(emb);
        createWaveformGeometry(emb);
      } else {
        console.warn("No embedding found for object:", suid);
        setEmbedding(null);
      }
    } catch (e) {
      console.error("Failed to fetch embedding:", e);
      // Show the actual error
      alert(`Error fetching embedding: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    if (controls) {
      controls.autoRotate = autoRotate();
    }
  });

  createEffect(() => {
    const emb = embedding();
    if (emb && scene) {
      createWaveformGeometry(emb);
    }
  });

  createEffect(() => {
    if (props.isOpen) {
      setTimeout(() => initThree(), 100);
    } else {
      cleanup();
    }
  });

  onCleanup(cleanup);

  const handleResize = () => {
    if (!containerRef || !camera || !renderer) return;
    const width = containerRef.clientWidth;
    const height = containerRef.clientHeight;
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    renderer.setSize(width, height);
  };

  onMount(() => {
    window.addEventListener("resize", handleResize);
  });

  onCleanup(() => {
    window.removeEventListener("resize", handleResize);
  });

  return (
    <Show when={props.isOpen}>
      <div class="waveform-3d-overlay">
        <div class="waveform-3d-modal">
          <div class="waveform-3d-header">
            <h2>3D Embedding Waveform</h2>
            <button class="close-btn" onClick={props.onClose}>×</button>
          </div>

          <div class="waveform-3d-content">
            {/* Sidebar */}
            <div class="waveform-3d-sidebar">
              {/* Search */}
              <div class="sidebar-section">
                <h4>Select Object</h4>
                <div class="search-row">
                  <input
                    type="text"
                    placeholder="Search..."
                    value={searchQuery()}
                    onInput={(e) => setSearchQuery(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleSearch()}
                  />
                  <button onClick={handleSearch} disabled={loading()}>
                    {loading() ? "..." : "Go"}
                  </button>
                </div>

                <Show when={searchResults().length > 0}>
                  <div class="search-results-list">
                    <For each={searchResults()}>
                      {(result) => (
                        <button
                          class={`result-btn ${selectedSuid() === result.suid ? "selected" : ""}`}
                          onClick={() => selectObject(result.suid)}
                        >
                          <span class="result-name">{result.name}</span>
                          <span class="result-score">{(result.score * 100).toFixed(0)}%</span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>

              {/* Display Mode */}
              <div class="sidebar-section">
                <h4>Display Mode</h4>
                <div class="mode-buttons">
                  <button
                    class={displayMode() === "ribbon" ? "active" : ""}
                    onClick={() => setDisplayMode("ribbon")}
                  >
                    Ribbon
                  </button>
                  <button
                    class={displayMode() === "tube" ? "active" : ""}
                    onClick={() => setDisplayMode("tube")}
                  >
                    Tube
                  </button>
                  <button
                    class={displayMode() === "surface" ? "active" : ""}
                    onClick={() => setDisplayMode("surface")}
                  >
                    Surface
                  </button>
                  <button
                    class={displayMode() === "helix" ? "active" : ""}
                    onClick={() => setDisplayMode("helix")}
                  >
                    Helix
                  </button>
                </div>
              </div>

              {/* Color Mode */}
              <div class="sidebar-section">
                <h4>Color Mode</h4>
                <div class="mode-buttons">
                  <button
                    class={colorMode() === "value" ? "active" : ""}
                    onClick={() => setColorMode("value")}
                  >
                    By Value
                  </button>
                  <button
                    class={colorMode() === "gradient" ? "active" : ""}
                    onClick={() => setColorMode("gradient")}
                  >
                    Gradient
                  </button>
                  <button
                    class={colorMode() === "position" ? "active" : ""}
                    onClick={() => setColorMode("position")}
                  >
                    Rainbow
                  </button>
                </div>
              </div>

              {/* Options */}
              <div class="sidebar-section">
                <label class="checkbox-label">
                  <input
                    type="checkbox"
                    checked={autoRotate()}
                    onChange={(e) => setAutoRotate(e.currentTarget.checked)}
                  />
                  Auto-rotate
                </label>
              </div>

              {/* Stats */}
              <Show when={embedding()}>
                <div class="sidebar-section stats-section">
                  <h4>Stats</h4>
                  <div class="stat-row">
                    <span>Dimensions:</span>
                    <span>{embedding()!.length}</span>
                  </div>
                  <div class="stat-row">
                    <span>Min:</span>
                    <span>{Math.min(...embedding()!).toFixed(4)}</span>
                  </div>
                  <div class="stat-row">
                    <span>Max:</span>
                    <span>{Math.max(...embedding()!).toFixed(4)}</span>
                  </div>
                  <div class="stat-row">
                    <span>Mean:</span>
                    <span>{(embedding()!.reduce((a, b) => a + b, 0) / embedding()!.length).toFixed(4)}</span>
                  </div>
                </div>
              </Show>
            </div>

            {/* 3D Viewport */}
            <div class="waveform-3d-viewport" ref={containerRef}>
              <Show when={!embedding()}>
                <div class="viewport-placeholder">
                  <p>Search and select an object to visualize its embedding as a 3D waveform</p>
                </div>
              </Show>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default EmbeddingWaveform3D;
