import { Component, createSignal, createEffect, onMount, onCleanup, Show, For } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import "./EmbeddingWaveform3D.css";

interface SearchResult {
  object: {
    id: string;
    kind: string;
    content: string;
    tags: string[];
  };
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
    console.log("initThree called, containerRef:", containerRef ? "exists" : "null");
    if (!containerRef) return;

    const width = containerRef.clientWidth;
    const height = containerRef.clientHeight;
    console.log("Container dimensions:", width, "x", height);

    // Scene
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x1a1a2e);

    // Camera
    camera = new THREE.PerspectiveCamera(60, width / height, 0.1, 1000);
    camera.position.set(0, 5, 15);
    camera.lookAt(0, 0, 0);
    console.log("Camera set up, looking at origin");

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(width, height);
    renderer.setPixelRatio(window.devicePixelRatio);
    containerRef.appendChild(renderer.domElement);

    // Controls
    controls = new OrbitControls(camera, renderer.domElement);
    controls.target.set(0, 0, 0);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.autoRotate = autoRotate();
    controls.autoRotateSpeed = 0.5;
    controls.update();

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

    // Test cube to verify rendering (named so we can identify it)
    const testGeometry = new THREE.BoxGeometry(2, 2, 2);
    const testMaterial = new THREE.MeshPhongMaterial({ color: 0xff0000 });
    const testCube = new THREE.Mesh(testGeometry, testMaterial);
    testCube.name = "testCube";
    testCube.position.set(0, 1, 0);
    scene.add(testCube);
    console.log("Added test cube at origin, total scene children:", scene.children.length);

    // Log all scene children for debugging
    scene.children.forEach((child, i) => {
      console.log(`  Child ${i}:`, child.type, child.name || "(unnamed)");
    });

    animate();
  };

  let frameCount = 0;
  const animate = () => {
    // Always schedule the next frame first to keep loop alive
    animationId = requestAnimationFrame(animate);

    if (!renderer || !scene || !camera || !controls) {
      if (frameCount % 60 === 0) {
        console.warn("animate: missing components", { renderer: !!renderer, scene: !!scene, camera: !!camera, controls: !!controls });
      }
      frameCount++;
      return;
    }

    try {
      controls.update();
      renderer.render(scene, camera);
    } catch (e) {
      console.error("Error in animate:", e);
    }

    frameCount++;
    if (frameCount === 1 || frameCount === 60 || frameCount === 120 || frameCount === 180) {
      console.log("Rendering frame", frameCount, "scene children:", scene.children.length,
        "camera:", camera.position.x.toFixed(1), camera.position.y.toFixed(1), camera.position.z.toFixed(1));
    }
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
    console.log("createWaveformGeometry called, scene:", scene ? "exists" : "null", "emb length:", emb.length);

    // Defensive check - ensure scene still exists
    if (!scene || !renderer || !camera) {
      console.warn("Scene/renderer/camera is null, cannot create geometry");
      return;
    }

    try {
      // Remove ONLY the existing waveform mesh/group, not other scene objects
      if (waveformMesh && scene.children.includes(waveformMesh)) {
        scene.remove(waveformMesh);
        // Dispose all children if it's a group
        if (waveformMesh instanceof THREE.Group) {
          waveformMesh.traverse((child) => {
            if (child instanceof THREE.Mesh || child instanceof THREE.Line) {
              child.geometry?.dispose();
              if (Array.isArray(child.material)) {
                child.material.forEach(m => m.dispose());
              } else if (child.material) {
                child.material.dispose();
              }
            }
          });
        } else if (waveformMesh instanceof THREE.Mesh || waveformMesh instanceof THREE.Line) {
          waveformMesh.geometry?.dispose();
          if (Array.isArray(waveformMesh.material)) {
            waveformMesh.material.forEach(m => m.dispose());
          } else if (waveformMesh.material) {
            waveformMesh.material.dispose();
          }
        }
        waveformMesh = null;
      }
      if (lineMesh && scene.children.includes(lineMesh)) {
        scene.remove(lineMesh);
        lineMesh.geometry?.dispose();
        if (Array.isArray(lineMesh.material)) {
          lineMesh.material.forEach(m => m.dispose());
        } else if (lineMesh.material) {
          lineMesh.material.dispose();
        }
        lineMesh = null;
      }

      console.log("Scene children after removal:", scene.children.length);

      // Normalize embedding
      let min = Infinity, max = -Infinity;
      for (const v of emb) {
        if (v < min) min = v;
        if (v > max) max = v;
      }
      const range = max - min || 1;
      console.log("Embedding range:", min, "to", max, "range:", range);

      // Create waveform as a ribbon with proper vertex colors
      const mode = displayMode();
      const len = emb.length;

      if (mode === "ribbon") {
        // Use 2D Canvas overlay for reliable waveform rendering
        console.log("Drawing waveform with Canvas2D, all", len, "points");

        // Find or create canvas overlay
        let waveCanvas = containerRef?.querySelector(".waveform-canvas") as HTMLCanvasElement | null;
        if (!waveCanvas) {
          waveCanvas = document.createElement("canvas");
          waveCanvas.className = "waveform-canvas";
          waveCanvas.style.cssText = "position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none;z-index:10;";
          containerRef?.appendChild(waveCanvas);
        }

        const rect = containerRef!.getBoundingClientRect();
        waveCanvas.width = rect.width * window.devicePixelRatio;
        waveCanvas.height = rect.height * window.devicePixelRatio;
        waveCanvas.style.width = rect.width + "px";
        waveCanvas.style.height = rect.height + "px";

        const ctx = waveCanvas.getContext("2d")!;
        ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
        ctx.clearRect(0, 0, rect.width, rect.height);

        const w = rect.width;
        const h = rect.height;
        const padding = 40;

        // Draw background
        ctx.fillStyle = "rgba(10, 10, 15, 0.85)";
        ctx.fillRect(padding, padding, w - padding * 2, h - padding * 2);

        // Draw center line
        ctx.strokeStyle = "rgba(100, 120, 160, 0.3)";
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(padding, h / 2);
        ctx.lineTo(w - padding, h / 2);
        ctx.stroke();

        // Draw waveform with all points
        const drawWidth = w - padding * 2;
        const drawHeight = (h - padding * 2) * 0.8;

        ctx.beginPath();
        for (let i = 0; i < len; i++) {
          const x = padding + (i / (len - 1)) * drawWidth;
          const normalized = (emb[i] - min) / range;
          const y = h / 2 - (normalized - 0.5) * drawHeight;

          if (i === 0) {
            ctx.moveTo(x, y);
          } else {
            ctx.lineTo(x, y);
          }
        }

        // Create gradient stroke
        const gradient = ctx.createLinearGradient(padding, 0, w - padding, 0);
        gradient.addColorStop(0, "rgb(74, 158, 255)");
        gradient.addColorStop(0.5, "rgb(100, 220, 180)");
        gradient.addColorStop(1, "rgb(180, 100, 255)");

        ctx.strokeStyle = gradient;
        ctx.lineWidth = 1.5;
        ctx.stroke();

        // Draw filled area under curve
        ctx.lineTo(w - padding, h / 2);
        ctx.lineTo(padding, h / 2);
        ctx.closePath();
        const fillGradient = ctx.createLinearGradient(0, padding, 0, h - padding);
        fillGradient.addColorStop(0, "rgba(74, 158, 255, 0.3)");
        fillGradient.addColorStop(0.5, "rgba(100, 220, 180, 0.1)");
        fillGradient.addColorStop(1, "rgba(180, 100, 255, 0.3)");
        ctx.fillStyle = fillGradient;
        ctx.fill();

        // Draw stats
        ctx.fillStyle = "#8899aa";
        ctx.font = "12px monospace";
        ctx.fillText(`dims: ${len}  min: ${min.toFixed(4)}  max: ${max.toFixed(4)}  range: ${range.toFixed(4)}`, padding, padding - 10);

        // Draw dimension markers
        ctx.fillStyle = "#556";
        ctx.font = "10px monospace";
        ctx.fillText("0", padding, h - padding + 15);
        ctx.fillText(String(Math.floor(len / 2)), w / 2, h - padding + 15);
        ctx.fillText(String(len - 1), w - padding - 20, h - padding + 15);

        console.log("Canvas waveform drawn with all", len, "dimensions");

        // Clean up Three.js elements we don't need for this mode
        const testCube = scene.getObjectByName("testCube");
        if (testCube) {
          scene.remove(testCube);
        }
        // Keep the 3D scene visible but behind the canvas

      } else if (mode === "tube") {
        // Clear any 2D canvas overlay
        const existingCanvas = containerRef?.querySelector(".waveform-canvas") as HTMLCanvasElement | null;
        if (existingCanvas) existingCanvas.remove();

        // Sample points for tube
        const sampleRate = Math.max(1, Math.floor(len / 500));
        const points: THREE.Vector3[] = [];

        for (let i = 0; i < len; i += sampleRate) {
          const x = (i / len) * 16 - 8;
          const normalized = (emb[i] - min) / range;
          const y = normalized * 4 - 2;
          const z = Math.sin((i / len) * Math.PI * 4) * 2;
          points.push(new THREE.Vector3(x, y, z));
        }

        const curve = new THREE.CatmullRomCurve3(points);
        const geometry = new THREE.TubeGeometry(curve, points.length, 0.1, 8, false);

        const material = new THREE.MeshPhongMaterial({
          color: 0x4a9eff,
          shininess: 100,
        });

        waveformMesh = new THREE.Mesh(geometry, material);
        scene.add(waveformMesh);
        console.log("Added tube mesh to scene");

      } else if (mode === "surface") {
        const segmentSize = Math.min(100, Math.ceil(Math.sqrt(len)));
        const geometry = new THREE.PlaneGeometry(16, 16, segmentSize - 1, segmentSize - 1);
        const positions = geometry.attributes.position.array as Float32Array;
        const colors: number[] = [];

        for (let i = 0; i < positions.length / 3; i++) {
          const embIdx = Math.floor((i / (positions.length / 3)) * len);
          const normalized = (emb[embIdx] - min) / range;
          positions[i * 3 + 2] = normalized * 4 - 2;

          const color = getColor(normalized, i / (positions.length / 3));
          colors.push(color.r, color.g, color.b);
        }

        geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
        geometry.computeVertexNormals();

        const material = new THREE.MeshPhongMaterial({
          vertexColors: true,
          side: THREE.DoubleSide,
        });

        waveformMesh = new THREE.Mesh(geometry, material);
        waveformMesh.rotation.x = -Math.PI / 4;
        scene.add(waveformMesh);
        console.log("Added surface mesh to scene");

      } else if (mode === "helix") {
        const geometry = new THREE.BufferGeometry();
        const vertices: number[] = [];
        const colors: number[] = [];
        const sampleRate = Math.max(1, Math.floor(len / 1000));

        for (let i = 0; i < len; i += sampleRate) {
          const t = i / len;
          const angle = t * Math.PI * 8;
          const radius = 3;
          const normalized = (emb[i] - min) / range;

          const x = Math.cos(angle) * radius;
          const y = (t - 0.5) * 16;
          const z = Math.sin(angle) * radius;

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
        const lineGeometry = geometry.clone();
        const lineMaterial = new THREE.LineBasicMaterial({
          vertexColors: true,
          opacity: 0.5,
          transparent: true,
        });
        lineMesh = new THREE.Line(lineGeometry, lineMaterial);
        scene.add(lineMesh);
        console.log("Added helix to scene");
      }

      // Force a render to see the changes immediately
      if (renderer && scene && camera) {
        renderer.render(scene, camera);
        console.log("Forced render after adding waveform, scene state:");
        scene.children.forEach((child, i) => {
          console.log(`  Child ${i}:`, child.type, child.name || "(unnamed)",
            child instanceof THREE.Mesh ? "visible:" + child.visible : "");
        });
      }

    } catch (e) {
      console.error("Error creating waveform geometry:", e);
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
      console.log("Invoking get_object_embedding with suid:", suid);
      const emb = await invoke<number[] | null>("get_object_embedding", { "suid": suid });
      console.log("Got embedding result:", emb ? `${emb.length} dimensions` : "null");

      if (emb && emb.length > 0) {
        console.log("Setting embedding...");
        setEmbedding(emb);
        // createWaveformGeometry will be called by the effect below
      } else {
        console.warn("No embedding found for object:", suid);
        setEmbedding(null);
      }
    } catch (e) {
      console.error("Failed to fetch embedding:", e);
      alert(`Error fetching embedding: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  // Update auto-rotate
  createEffect(() => {
    if (controls) {
      controls.autoRotate = autoRotate();
    }
  });

  // Recreate waveform when embedding or display mode changes
  createEffect(() => {
    const emb = embedding();
    const mode = displayMode(); // Track display mode changes too
    console.log("Effect triggered - embedding:", emb ? emb.length : "null", "mode:", mode, "scene:", scene ? "exists" : "null");
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
                          class={`result-btn ${selectedSuid() === result.object.id ? "selected" : ""}`}
                          onClick={() => selectObject(result.object.id)}
                        >
                          <span class="result-name">{result.object.content.slice(0, 50)}...</span>
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
