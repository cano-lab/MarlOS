import { Component, createSignal, Show } from "solid-js";
import MobileCapture from "./MobileCapture";
import MobileMemoryViewer from "./MobileMemoryViewer";
import MobileResearch from "./MobileResearch";
import "./MobileApp.css";

type MobileTab = "memory" | "capture" | "research" | "settings";

const MobileApp: Component = () => {
  const [activeTab, setActiveTab] = createSignal<MobileTab>("memory");
  const [showCaptureModal, setShowCaptureModal] = createSignal(false);

  const handleCaptureComplete = (id: string) => {
    console.log("Captured:", id);
    setShowCaptureModal(false);
    // Switch to memory view to see the new capture
    setActiveTab("memory");
  };

  return (
    <div class="mobile-app">
      {/* Main Content Area */}
      <div class="mobile-content">
        <Show when={activeTab() === "memory"}>
          <MobileMemoryViewer />
        </Show>

        <Show when={activeTab() === "capture"}>
          <MobileCapture onCapture={handleCaptureComplete} />
        </Show>

        <Show when={activeTab() === "research"}>
          <MobileResearch />
        </Show>

        <Show when={activeTab() === "settings"}>
          <MobileSettings />
        </Show>
      </div>

      {/* Bottom Navigation */}
      <nav class="bottom-nav">
        <button
          class={`nav-btn ${activeTab() === "memory" ? "active" : ""}`}
          onClick={() => setActiveTab("memory")}
        >
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-width="1.5" />
            <path d="M12 7v5l3 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
          <span>Memory</span>
        </button>

        <button
          class={`nav-btn capture-btn ${activeTab() === "capture" ? "active" : ""}`}
          onClick={() => setActiveTab("capture")}
        >
          <div class="capture-icon">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
              <path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" />
            </svg>
          </div>
          <span>Capture</span>
        </button>

        <button
          class={`nav-btn ${activeTab() === "research" ? "active" : ""}`}
          onClick={() => setActiveTab("research")}
        >
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <circle cx="10" cy="10" r="6" stroke="currentColor" stroke-width="1.5" />
            <path d="M14 14l5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
            <path d="M8 10h4M10 8v4" stroke="currentColor" stroke-width="1" />
          </svg>
          <span>Research</span>
        </button>

        <button
          class={`nav-btn ${activeTab() === "settings" ? "active" : ""}`}
          onClick={() => setActiveTab("settings")}
        >
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.5" />
            <path
              d="M12 2v2m0 16v2M2 12h2m16 0h2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M4.93 19.07l1.41-1.41m11.32-11.32l1.41-1.41"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </svg>
          <span>Settings</span>
        </button>
      </nav>

      {/* Quick Capture FAB (alternative entry point) */}
      <Show when={activeTab() !== "capture"}>
        <button class="capture-fab" onClick={() => setShowCaptureModal(true)}>
          <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
            <path d="M14 6v16M6 14h16" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
          </svg>
        </button>
      </Show>

      {/* Capture Modal Overlay */}
      <Show when={showCaptureModal()}>
        <div class="capture-modal-overlay">
          <div class="capture-modal">
            <MobileCapture
              onCapture={handleCaptureComplete}
              onClose={() => setShowCaptureModal(false)}
            />
          </div>
        </div>
      </Show>
    </div>
  );
};

// Settings component
const MobileSettings: Component = () => {
  return (
    <div class="mobile-settings">
      <div class="settings-header">
        <h2>Settings</h2>
      </div>

      <div class="settings-content">
        <div class="settings-section">
          <h3>Sync</h3>
          <div class="setting-item">
            <span class="setting-label">Sync Status</span>
            <span class="setting-value connected">Connected</span>
          </div>
          <div class="setting-item">
            <span class="setting-label">Last Sync</span>
            <span class="setting-value">Just now</span>
          </div>
        </div>

        <div class="settings-section">
          <h3>Storage</h3>
          <div class="setting-item">
            <span class="setting-label">Memory Objects</span>
            <span class="setting-value">0</span>
          </div>
          <div class="setting-item">
            <span class="setting-label">Cache Size</span>
            <span class="setting-value">0 MB</span>
          </div>
        </div>

        <div class="settings-section">
          <h3>Account</h3>
          <button class="setting-btn">Manage Subscription</button>
          <button class="setting-btn">Export Data</button>
          <button class="setting-btn danger">Sign Out</button>
        </div>

        <div class="settings-section">
          <h3>About</h3>
          <div class="setting-item">
            <span class="setting-label">Version</span>
            <span class="setting-value">1.0.0</span>
          </div>
        </div>
      </div>
    </div>
  );
};

export default MobileApp;
