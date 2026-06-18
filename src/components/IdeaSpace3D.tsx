import { createSignal, createEffect, onCleanup, Show, For, type Component } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import "./IdeaSpace3D.css";

interface IdeaSpacePoint {
  id: string;
  name: string;
  object_type: string;
  x: number;
  y: number;
  z: number;
  distance_from_center: number;
  magnitude: number;
  tags: string[];
  preview: string;
}

interface SemanticAxis {
  name: string;
  negative_label: string;
  positive_label: string;
}

interface SpaceBounds {
  min_x: number;
  max_x: number;
  min_y: number;
  max_y: number;
  min_z: number;
  max_z: number;
}

interface IdeaSpace {
  points: IdeaSpacePoint[];
  axes: SemanticAxis[];
  variance_captured: number;
  bounds: SpaceBounds;
}

interface ClusterInfo {
  id: number;
  centroid: { x: number; y: number; z: number };
  member_count: number;
  common_tags: { tag: string; count: number }[];
  sample_members: { id: string; name: string; type: string }[];
}

interface MeanDistanceResult {
  id: string;
  name: string;
  object_type: string;
  distance: number;
  preview: string;
  tags: string[];
}

interface MeanAnalysis {
  closest_to_mean: MeanDistanceResult[];
  farthest_from_mean: MeanDistanceResult[];
  mean_summary: string;
  total_objects: number;
  avg_distance: number;
}

interface TrackedRepo {
  id: string;
  path: string;
  name: string;
  first_imported: string;
  last_synced: string;
  last_commit: string | null;
  file_count: number;
  object_count: number;
  auto_sync: string | { Scheduled: { interval_hours: number } };
}

// RepoStatus interface reserved for future use

interface IdeaSpace3DProps {
  isOpen: boolean;
  onClose: () => void;
}

const IdeaSpace3D: Component<IdeaSpace3DProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let renderer: THREE.WebGLRenderer | null = null;
  let scene: THREE.Scene | null = null;
  let camera: THREE.PerspectiveCamera | null = null;
  let controls: OrbitControls | null = null;
  let animationId: number | null = null;
  let pointsGroup: THREE.Group | null = null;
  let raycaster: THREE.Raycaster | null = null;
  let mouse: THREE.Vector2 | null = null;
  let pointMeshes: Map<string, THREE.Mesh> = new Map();

  const [loading, setLoading] = createSignal(true);
  const [loadingStatus, setLoadingStatus] = createSignal("Initializing...");
  const [loadingPercent, setLoadingPercent] = createSignal(0);
  const [error, setError] = createSignal<string | null>(null);
  let progressUnlisten: UnlistenFn | null = null;
  const [space, setSpace] = createSignal<IdeaSpace | null>(null);
  const [totalObjects, setTotalObjects] = createSignal(0);
  const [selectedPoint, setSelectedPoint] = createSignal<IdeaSpacePoint | null>(null);
  const [hoveredPoint, setHoveredPoint] = createSignal<IdeaSpacePoint | null>(null);
  const [_viewMode, _setViewMode] = createSignal<"pca" | "custom">("pca");
  const [clusters, setClusters] = createSignal<ClusterInfo[]>([]);
  const [showClusters, setShowClusters] = createSignal(false);
  const [filterType, setFilterType] = createSignal<string | null>(null);
  const [showMeanAnalysis, setShowMeanAnalysis] = createSignal(false);
  const [meanAnalysis, setMeanAnalysis] = createSignal<MeanAnalysis | null>(null);
  const [showRepoImport, setShowRepoImport] = createSignal(false);
  const [repoPath, setRepoPath] = createSignal("");
  const [importing, setImporting] = createSignal(false);
  const [importStatus, setImportStatus] = createSignal("");
  const [trackedRepos, setTrackedRepos] = createSignal<TrackedRepo[]>([]);
  const [_showRepoList, _setShowRepoList] = createSignal(false);
  const [projectionMode, setProjectionMode] = createSignal<string>("pca");

  // Load tracked repos on open
  const loadTrackedRepos = async () => {
    try {
      const repos = await invoke<TrackedRepo[]>("repo_list");
      setTrackedRepos(repos);
    } catch (e) {
      console.warn("Failed to load tracked repos:", e);
    }
  };

  // Check if repo already tracked
  const checkRepoTracked = async (path: string): Promise<TrackedRepo | null> => {
    try {
      return await invoke<TrackedRepo | null>("repo_check_path", { path });
    } catch {
      return null;
    }
  };

  // Sync a tracked repo
  const syncRepo = async (id: string) => {
    setImporting(true);
    setImportStatus("Syncing repository...");
    try {
      const result = await invoke<string>("repo_sync", { id });
      setImportStatus(result);
      await loadTrackedRepos();
      setTimeout(() => loadIdeaSpace(), 2000);
    } catch (e) {
      setImportStatus(`Error: ${e}`);
    } finally {
      setImporting(false);
    }
  };

  // Color mapping for object types
  const typeColors: Record<string, number> = {
    "text": 0x4a9eff,       // Blue
    "markdown": 0x4aff9e,   // Green
    "json": 0xffdd4a,       // Yellow
    "code": 0xff4a8a,       // Pink
    "unknown": 0x888888,    // Gray
  };

  const getPointColor = (type: string): number => {
    // Check for code: prefix
    if (type.startsWith("code:")) return typeColors["code"];
    if (type.startsWith("binary:")) return 0xaa88ff;  // Purple
    if (type.startsWith("structured:")) return 0xff8844; // Orange
    return typeColors[type] || typeColors["unknown"];
  };

  const loadIdeaSpace = async () => {
    setLoading(true);
    setError(null);
    setLoadingPercent(0);
    setLoadingStatus("Checking vector database...");

    // Listen for progress events from backend
    if (progressUnlisten) {
      progressUnlisten();
    }
    progressUnlisten = await listen<{ stage: string; message: string; percent: number }>(
      "idea-space-progress",
      (event) => {
        setLoadingStatus(event.payload.message);
        setLoadingPercent(event.payload.percent);
      }
    );

    try {
      // First, get a count of objects to show progress
      try {
        const allObjects = await invoke<any[]>("get_all_vector_objects", {
          limit: 999999,  // No practical limit - get everything
        });
        setTotalObjects(allObjects.length);

        if (allObjects.length === 0) {
          setError("No objects in vector database. Index some sessions first using the Database Settings.");
          setLoading(false);
          return;
        }

        if (allObjects.length < 3) {
          setError(`Need at least 3 objects for 3D projection. Currently have ${allObjects.length}.`);
          setLoading(false);
          return;
        }

        setLoadingStatus(`Found ${allObjects.length} objects. Computing PCA projection...`);
      } catch (e) {
        console.warn("Could not get object count:", e);
        setLoadingStatus("Loading vectors...");
      }

      // Now load the 3D projection
      const mode = projectionMode();
      setLoadingStatus(`Projecting ${totalObjects()} vectors to 3D (${mode})...`);
      const data = await invoke<IdeaSpace>("get_idea_space_3d", {
        projectionMode: mode === "pca" ? null : mode,
      });
      setSpace(data);
      setLoadingStatus(`Loaded ${data.points.length} points. Finding clusters...`);

      // Also load clusters
      try {
        const clusterData = await invoke<{ clusters: ClusterInfo[] }>("get_idea_clusters", {
          numClusters: 5,
        });
        setClusters(clusterData.clusters);
      } catch (e) {
        console.warn("Failed to load clusters:", e);
      }

      setLoadingStatus("Rendering...");
    } catch (e) {
      setError(`Failed to load 3D space: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const loadMeanAnalysis = async () => {
    try {
      const analysis = await invoke<MeanAnalysis>("analyze_knowledge_center", { limit: 10 });
      setMeanAnalysis(analysis);
      setShowMeanAnalysis(true);
    } catch (e) {
      console.error("Failed to load mean analysis:", e);
      alert(`Failed to analyze: ${e}`);
    }
  };

  const importRepository = async () => {
    if (!repoPath().trim()) {
      alert("Please enter a repository path");
      return;
    }

    setImporting(true);
    setImportStatus("Starting import...");

    // Listen for progress events
    const unlisten = await listen<{ stage: string; message: string; imported: number; total: number }>(
      "repo-import-progress",
      (event) => {
        setImportStatus(event.payload.message);
      }
    );

    try {
      // Check if already tracked
      const existing = await checkRepoTracked(repoPath());

      if (existing) {
        // Just sync the existing repo
        setImportStatus("Repository already tracked, syncing...");
        await syncRepo(existing.id);
      } else {
        // Import new repo
        const result = await invoke<string>("import_repository", { path: repoPath() });
        setImportStatus(result);

        // Register for tracking
        try {
          await invoke("repo_add", { path: repoPath(), name: null });
          await loadTrackedRepos();
          setImportStatus(result + " (Now tracking for updates)");
        } catch (e) {
          console.warn("Failed to register repo for tracking:", e);
        }
      }

      // Reload the space after import
      setTimeout(() => {
        setShowRepoImport(false);
        setRepoPath("");
        loadIdeaSpace();
      }, 2000);
    } catch (e) {
      setImportStatus(`Error: ${e}`);
    } finally {
      setImporting(false);
      unlisten();
    }
  };

  const initThreeJS = () => {
    if (!containerRef || !space()) return;

    // Scene
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0a0a0f);

    // Camera
    const width = containerRef.clientWidth;
    const height = containerRef.clientHeight;
    camera = new THREE.PerspectiveCamera(60, width / height, 0.1, 1000);
    camera.position.set(2.5, 2, 2.5);

    // Renderer
    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(width, height);
    renderer.setPixelRatio(window.devicePixelRatio);
    containerRef.appendChild(renderer.domElement);

    // Controls
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.minDistance = 1;
    controls.maxDistance = 10;

    // Raycaster for mouse picking
    raycaster = new THREE.Raycaster();
    mouse = new THREE.Vector2();

    // Lighting
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.6);
    scene.add(ambientLight);

    const directionalLight = new THREE.DirectionalLight(0xffffff, 0.8);
    directionalLight.position.set(5, 5, 5);
    scene.add(directionalLight);

    // Grid helper
    const gridHelper = new THREE.GridHelper(4, 20, 0x333344, 0x222233);
    gridHelper.position.y = -1.5;
    scene.add(gridHelper);

    // Axes labels
    addAxesLabels();

    // Points
    pointsGroup = new THREE.Group();
    scene.add(pointsGroup);
    updatePoints();

    // Event listeners
    renderer.domElement.addEventListener("mousemove", onMouseMove);
    renderer.domElement.addEventListener("click", onClick);
    window.addEventListener("resize", onResize);

    // Animation loop
    animate();
  };

  const addAxesLabels = () => {
    if (!scene) return;

    const axisLength = 1.5;
    void (space()?.axes || []);

    // X axis (red)
    const xGeometry = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(-axisLength, 0, 0),
      new THREE.Vector3(axisLength, 0, 0),
    ]);
    const xMaterial = new THREE.LineBasicMaterial({ color: 0xff4444 });
    const xLine = new THREE.Line(xGeometry, xMaterial);
    scene.add(xLine);

    // Y axis (green)
    const yGeometry = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(0, -axisLength, 0),
      new THREE.Vector3(0, axisLength, 0),
    ]);
    const yMaterial = new THREE.LineBasicMaterial({ color: 0x44ff44 });
    const yLine = new THREE.Line(yGeometry, yMaterial);
    scene.add(yLine);

    // Z axis (blue)
    const zGeometry = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(0, 0, -axisLength),
      new THREE.Vector3(0, 0, axisLength),
    ]);
    const zMaterial = new THREE.LineBasicMaterial({ color: 0x4444ff });
    const zLine = new THREE.Line(zGeometry, zMaterial);
    scene.add(zLine);
  };

  const updatePoints = () => {
    if (!pointsGroup || !space()) return;

    // Clear existing points
    while (pointsGroup.children.length > 0) {
      pointsGroup.remove(pointsGroup.children[0]);
    }
    pointMeshes.clear();

    const points = space()!.points;
    const filter = filterType();

    for (const point of points) {
      // Apply type filter
      if (filter && !point.object_type.includes(filter)) continue;

      const geometry = new THREE.SphereGeometry(0.03, 16, 16);
      const color = getPointColor(point.object_type);
      const material = new THREE.MeshPhongMaterial({
        color,
        emissive: color,
        emissiveIntensity: 0.2,
      });

      const mesh = new THREE.Mesh(geometry, material);
      mesh.position.set(point.x, point.y, point.z);
      mesh.userData = point;

      pointsGroup.add(mesh);
      pointMeshes.set(point.id, mesh);
    }

    // Add cluster centers if enabled
    if (showClusters()) {
      for (const cluster of clusters()) {
        const geometry = new THREE.SphereGeometry(0.08, 8, 8);
        const material = new THREE.MeshBasicMaterial({
          color: 0xffff00,
          wireframe: true,
        });

        const mesh = new THREE.Mesh(geometry, material);
        mesh.position.set(cluster.centroid.x, cluster.centroid.y, cluster.centroid.z);
        pointsGroup.add(mesh);
      }
    }
  };

  const onMouseMove = (event: MouseEvent) => {
    if (!containerRef || !raycaster || !mouse || !camera || !pointsGroup) return;

    const rect = containerRef.getBoundingClientRect();
    mouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    mouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;

    raycaster.setFromCamera(mouse, camera);
    const intersects = raycaster.intersectObjects(pointsGroup.children);

    if (intersects.length > 0) {
      const point = intersects[0].object.userData as IdeaSpacePoint;
      if (point.id) {
        setHoveredPoint(point);
        containerRef.style.cursor = "pointer";
      }
    } else {
      setHoveredPoint(null);
      containerRef.style.cursor = "grab";
    }
  };

  const onClick = (event: MouseEvent) => {
    if (!containerRef || !raycaster || !mouse || !camera || !pointsGroup) return;

    const rect = containerRef.getBoundingClientRect();
    mouse.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    mouse.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;

    raycaster.setFromCamera(mouse, camera);
    const intersects = raycaster.intersectObjects(pointsGroup.children);

    if (intersects.length > 0) {
      const point = intersects[0].object.userData as IdeaSpacePoint;
      if (point.id) {
        setSelectedPoint(point);
      }
    }
  };

  const onResize = () => {
    if (!containerRef || !camera || !renderer) return;

    const width = containerRef.clientWidth;
    const height = containerRef.clientHeight;

    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    renderer.setSize(width, height);
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

    if (renderer) {
      renderer.domElement.removeEventListener("mousemove", onMouseMove);
      renderer.domElement.removeEventListener("click", onClick);
      renderer.dispose();
      if (containerRef && renderer.domElement.parentElement) {
        containerRef.removeChild(renderer.domElement);
      }
    }

    window.removeEventListener("resize", onResize);

    renderer = null;
    scene = null;
    camera = null;
    controls = null;
    pointsGroup = null;
    raycaster = null;
    mouse = null;
    pointMeshes.clear();
  };

  const focusOnPoint = (point: IdeaSpacePoint) => {
    if (!camera || !controls) return;

    const targetPosition = new THREE.Vector3(point.x, point.y, point.z);
    const cameraOffset = new THREE.Vector3(0.5, 0.5, 0.5);

    camera.position.copy(targetPosition).add(cameraOffset);
    controls.target.copy(targetPosition);
    controls.update();
  };

  const resetCamera = () => {
    if (!camera || !controls) return;

    camera.position.set(2.5, 2, 2.5);
    controls.target.set(0, 0, 0);
    controls.update();
  };

  // Watch for filter/cluster changes
  const handleFilterChange = (type: string | null) => {
    setFilterType(type);
    updatePoints();
  };

  const handleClusterToggle = () => {
    setShowClusters(!showClusters());
    updatePoints();
  };

  onCleanup(() => {
    cleanup();
    if (progressUnlisten) {
      progressUnlisten();
      progressUnlisten = null;
    }
  });

  // Re-initialize when opened - use createEffect for reactivity
  createEffect(async () => {
    if (props.isOpen && !scene) {
      await loadTrackedRepos();
      await loadIdeaSpace();
      // Small delay to ensure container is rendered
      setTimeout(() => initThreeJS(), 100);
    }
  });

  const objectTypes = () => {
    const types = new Set<string>();
    space()?.points.forEach((p) => {
      const baseType = p.object_type.split(":")[0];
      types.add(baseType);
    });
    return Array.from(types);
  };

  return (
    <Show when={props.isOpen}>
      <div class="idea-space-overlay">
        <div class="idea-space-container">
          {/* Header */}
          <div class="idea-space-header">
            <h2>3D Idea Space</h2>
            <div class="header-controls">
              <Show when={space()}>
                <span class="point-count">{space()!.points.length} objects</span>
                <span class="variance-info">
                  {(space()!.variance_captured * 100).toFixed(1)}% variance captured
                </span>
              </Show>
              <button class="close-btn" onClick={props.onClose}>✕</button>
            </div>
          </div>

          {/* Main content */}
          <div class="idea-space-content">
            {/* Sidebar */}
            <div class="idea-space-sidebar">
              {/* View controls */}
              <div class="control-section">
                <h4>View</h4>
                <button class="control-btn" onClick={resetCamera}>Reset Camera</button>
                <button
                  class={`control-btn ${showClusters() ? "active" : ""}`}
                  onClick={handleClusterToggle}
                >
                  {showClusters() ? "Hide" : "Show"} Clusters
                </button>
              </div>

              {/* Projection Mode */}
              <div class="control-section">
                <h4>Projection Mode</h4>
                <select
                  class="projection-select"
                  value={projectionMode()}
                  onChange={async (e) => {
                    setProjectionMode(e.currentTarget.value);
                    cleanup();
                    await loadIdeaSpace();
                    setTimeout(() => initThreeJS(), 100);
                  }}
                >
                  <option value="pca">PCA (variance)</option>
                  <option value="folded">Folded (sum)</option>
                  <option value="folded_mean">Folded (mean)</option>
                  <option value="folded_max">Folded (max)</option>
                  <option value="folded_variance">Folded (variance)</option>
                  <option value="folded_l2">Folded (L2 norm)</option>
                </select>
                <p class="projection-hint">
                  {projectionMode() === "pca"
                    ? "Maximizes variance separation"
                    : "Chunks 1024D into 3 parts"}
                </p>
              </div>

              {/* Analysis */}
              <div class="control-section">
                <h4>Analysis</h4>
                <button class="control-btn analyze-btn" onClick={loadMeanAnalysis}>
                  🎯 Knowledge Center
                </button>
                <button class="control-btn import-btn" onClick={() => setShowRepoImport(true)}>
                  📁 Import Repository
                </button>
              </div>

              {/* Tracked Repositories */}
              <Show when={trackedRepos().length > 0}>
                <div class="control-section">
                  <h4>Tracked Repos ({trackedRepos().length})</h4>
                  <div class="repo-list">
                    <For each={trackedRepos()}>
                      {(repo) => (
                        <div class="repo-item">
                          <span class="repo-name" title={repo.path}>{repo.name}</span>
                          <span class="repo-count">{repo.object_count}</span>
                          <button
                            class="repo-sync-btn"
                            onClick={() => syncRepo(repo.id)}
                            disabled={importing()}
                            title="Sync repository"
                          >
                            🔄
                          </button>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              {/* Type filter */}
              <div class="control-section">
                <h4>Filter by Type</h4>
                <button
                  class={`filter-btn ${filterType() === null ? "active" : ""}`}
                  onClick={() => handleFilterChange(null)}
                >
                  All
                </button>
                <For each={objectTypes()}>
                  {(type) => (
                    <button
                      class={`filter-btn ${filterType() === type ? "active" : ""}`}
                      onClick={() => handleFilterChange(type)}
                    >
                      {type}
                    </button>
                  )}
                </For>
              </div>

              {/* Axes info */}
              <div class="control-section">
                <h4>Axes ({projectionMode() === "pca" ? "PCA" : "Folded"})</h4>
                <div class="axis-info">
                  <Show when={projectionMode() === "pca"}>
                    <div class="axis-row">
                      <span class="axis-color x"></span>
                      <span>X: Primary variance</span>
                    </div>
                    <div class="axis-row">
                      <span class="axis-color y"></span>
                      <span>Y: Secondary variance</span>
                    </div>
                    <div class="axis-row">
                      <span class="axis-color z"></span>
                      <span>Z: Tertiary variance</span>
                    </div>
                  </Show>
                  <Show when={projectionMode() !== "pca"}>
                    <div class="axis-row">
                      <span class="axis-color x"></span>
                      <span>X: Dims 1-341</span>
                    </div>
                    <div class="axis-row">
                      <span class="axis-color y"></span>
                      <span>Y: Dims 342-682</span>
                    </div>
                    <div class="axis-row">
                      <span class="axis-color z"></span>
                      <span>Z: Dims 683-1024</span>
                    </div>
                  </Show>
                </div>
              </div>

              {/* Legend */}
              <div class="control-section">
                <h4>Legend</h4>
                <div class="legend">
                  <div class="legend-item">
                    <span class="legend-color" style={{ background: "#4a9eff" }}></span>
                    <span>Text</span>
                  </div>
                  <div class="legend-item">
                    <span class="legend-color" style={{ background: "#4aff9e" }}></span>
                    <span>Markdown</span>
                  </div>
                  <div class="legend-item">
                    <span class="legend-color" style={{ background: "#ffdd4a" }}></span>
                    <span>JSON</span>
                  </div>
                  <div class="legend-item">
                    <span class="legend-color" style={{ background: "#ff4a8a" }}></span>
                    <span>Code</span>
                  </div>
                  <div class="legend-item">
                    <span class="legend-color" style={{ background: "#aa88ff" }}></span>
                    <span>Binary</span>
                  </div>
                </div>
              </div>

              {/* Clusters */}
              <Show when={showClusters() && clusters().length > 0}>
                <div class="control-section">
                  <h4>Clusters</h4>
                  <div class="cluster-list">
                    <For each={clusters()}>
                      {(cluster) => (
                        <div class="cluster-item">
                          <span class="cluster-count">{cluster.member_count}</span>
                          <span class="cluster-tags">
                            {cluster.common_tags.slice(0, 2).map((t) => t.tag).join(", ") || "No tags"}
                          </span>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </Show>
            </div>

            {/* 3D Canvas */}
            <div class="canvas-container">
              <Show when={loading()}>
                <div class="loading-overlay">
                  <div class="loading-spinner"></div>
                  <p class="loading-status">{loadingStatus()}</p>
                  <Show when={totalObjects() > 0}>
                    <p class="loading-count">{totalObjects()} vectors total</p>
                  </Show>
                  <Show when={loadingPercent() > 0}>
                    <div class="progress-bar-container">
                      <div class="progress-bar" style={{ width: `${loadingPercent()}%` }}></div>
                    </div>
                    <p class="loading-percent">{loadingPercent()}%</p>
                  </Show>
                </div>
              </Show>

              <Show when={error()}>
                <div class="error-overlay">
                  <p>{error()}</p>
                  <button onClick={loadIdeaSpace}>Retry</button>
                </div>
              </Show>

              <div ref={containerRef} class="three-canvas"></div>

              {/* Hover tooltip */}
              <Show when={hoveredPoint()}>
                <div class="hover-tooltip">
                  <div class="tooltip-title">{hoveredPoint()!.name}</div>
                  <div class="tooltip-type">{hoveredPoint()!.object_type}</div>
                </div>
              </Show>
            </div>
          </div>

          {/* Selected point detail */}
          <Show when={selectedPoint()}>
            <div class="selected-panel">
              <div class="selected-header">
                <h3>{selectedPoint()!.name}</h3>
                <button class="close-btn small" onClick={() => setSelectedPoint(null)}>✕</button>
              </div>
              <div class="selected-body">
                <div class="selected-meta">
                  <span class="meta-type">{selectedPoint()!.object_type}</span>
                  <Show when={selectedPoint()!.tags.length > 0}>
                    <div class="meta-tags">
                      <For each={selectedPoint()!.tags.slice(0, 5)}>
                        {(tag) => <span class="tag">{tag}</span>}
                      </For>
                    </div>
                  </Show>
                </div>
                <div class="selected-preview">{selectedPoint()!.preview}</div>
                <div class="selected-coords">
                  Position: ({selectedPoint()!.x.toFixed(2)}, {selectedPoint()!.y.toFixed(2)}, {selectedPoint()!.z.toFixed(2)})
                </div>
                <div class="selected-actions">
                  <button class="action-btn" onClick={() => focusOnPoint(selectedPoint()!)}>
                    Focus
                  </button>
                </div>
              </div>
            </div>
          </Show>

          {/* Mean Analysis Panel */}
          <Show when={showMeanAnalysis() && meanAnalysis()}>
            <div class="analysis-panel">
              <div class="analysis-header">
                <h3>🎯 Knowledge Center Analysis</h3>
                <button class="close-btn small" onClick={() => setShowMeanAnalysis(false)}>✕</button>
              </div>
              <div class="analysis-body">
                <p class="analysis-summary">{meanAnalysis()!.mean_summary}</p>
                <p class="analysis-stats">
                  {meanAnalysis()!.total_objects.toLocaleString()} objects analyzed •
                  Avg distance: {meanAnalysis()!.avg_distance.toFixed(3)}
                </p>

                <div class="analysis-section">
                  <h4>🎯 Most Typical (Closest to Center)</h4>
                  <div class="analysis-list">
                    <For each={meanAnalysis()!.closest_to_mean}>
                      {(item) => (
                        <div class="analysis-item typical">
                          <div class="item-name">{item.name}</div>
                          <div class="item-meta">
                            <span class="item-type">{item.object_type}</span>
                            <span class="item-distance">d={item.distance.toFixed(3)}</span>
                          </div>
                          <div class="item-preview">{item.preview}</div>
                        </div>
                      )}
                    </For>
                  </div>
                </div>

                <div class="analysis-section">
                  <h4>🌟 Most Unique (Farthest from Center)</h4>
                  <div class="analysis-list">
                    <For each={meanAnalysis()!.farthest_from_mean}>
                      {(item) => (
                        <div class="analysis-item unique">
                          <div class="item-name">{item.name}</div>
                          <div class="item-meta">
                            <span class="item-type">{item.object_type}</span>
                            <span class="item-distance">d={item.distance.toFixed(3)}</span>
                          </div>
                          <div class="item-preview">{item.preview}</div>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </div>
            </div>
          </Show>

          {/* Repo Import Dialog */}
          <Show when={showRepoImport()}>
            <div class="import-dialog-overlay" onClick={() => !importing() && setShowRepoImport(false)}>
              <div class="import-dialog" onClick={(e) => e.stopPropagation()}>
                <div class="import-header">
                  <h3>📁 Import Repository</h3>
                  <button
                    class="close-btn small"
                    onClick={() => setShowRepoImport(false)}
                    disabled={importing()}
                  >✕</button>
                </div>
                <div class="import-body">
                  <p>Import all code files from a repository into the vector database.</p>
                  <input
                    type="text"
                    class="import-input"
                    placeholder="C:\path\to\your\repo"
                    value={repoPath()}
                    onInput={(e) => setRepoPath(e.currentTarget.value)}
                    disabled={importing()}
                  />
                  <Show when={importStatus()}>
                    <p class="import-status">{importStatus()}</p>
                  </Show>
                  <div class="import-actions">
                    <button
                      class="import-cancel-btn"
                      onClick={() => setShowRepoImport(false)}
                      disabled={importing()}
                    >
                      Cancel
                    </button>
                    <button
                      class="import-confirm-btn"
                      onClick={importRepository}
                      disabled={importing() || !repoPath().trim()}
                    >
                      {importing() ? "Importing..." : "Import"}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

export default IdeaSpace3D;
