import { Component, createSignal, Show, For } from "solid-js";
import "./LocationPicker.css";

export interface SavedLocation {
  id: string;
  name: string;
  x: number;
  y: number;
  zoom: number;
  createdAt: Date;
}

interface LocationPickerProps {
  locations: SavedLocation[];
  onSaveLocation: (name: string) => void;
  onJumpTo: (location: SavedLocation) => void;
  onDelete: (id: string) => void;
  onGoHome: () => void;
}

const LocationPicker: Component<LocationPickerProps> = (props) => {
  const [isOpen, setIsOpen] = createSignal(false);
  const [isSaving, setIsSaving] = createSignal(false);
  const [newLocationName, setNewLocationName] = createSignal("");

  const handleSave = () => {
    const name = newLocationName().trim();
    if (name) {
      props.onSaveLocation(name);
      setNewLocationName("");
      setIsSaving(false);
    }
  };

  const handleKeyPress = (e: KeyboardEvent) => {
    if (e.key === "Enter") handleSave();
    if (e.key === "Escape") {
      setIsSaving(false);
      setNewLocationName("");
    }
  };

  return (
    <div class="location-picker">
      <button
        class="location-btn"
        onClick={() => setIsOpen(!isOpen())}
        title="Saved locations"
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path
            d="M7 1L9 5H13L10 8L11 12L7 9L3 12L4 8L1 5H5L7 1Z"
            fill="currentColor"
            stroke="currentColor"
            stroke-width="0.5"
            stroke-linejoin="round"
          />
        </svg>
        Locations
        <span class="location-count">{props.locations.length}</span>
      </button>

      <Show when={isOpen()}>
        <div class="location-dropdown">
          <div class="location-dropdown-header">
            <button class="location-home-btn" onClick={() => { props.onGoHome(); setIsOpen(false); }}>
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                <path d="M6 1L1 5v6h2V7h6v4h2V5L6 1z" stroke="currentColor" stroke-width="1" stroke-linejoin="round"/>
              </svg>
              Home
            </button>
            <button
              class="location-save-btn"
              onClick={() => setIsSaving(true)}
              title="Save current location"
            >
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                <path d="M6 1v10M1 6h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
              Save
            </button>
          </div>

          <Show when={isSaving()}>
            <div class="location-save-form">
              <input
                type="text"
                class="location-name-input"
                placeholder="Location name..."
                value={newLocationName()}
                onInput={(e) => setNewLocationName(e.currentTarget.value)}
                onKeyDown={handleKeyPress}
                autofocus
              />
              <div class="location-save-actions">
                <button class="location-save-confirm" onClick={handleSave}>
                  Save
                </button>
                <button
                  class="location-save-cancel"
                  onClick={() => {
                    setIsSaving(false);
                    setNewLocationName("");
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          </Show>

          <Show when={!isSaving() && props.locations.length > 0}>
            <div class="location-list">
              <For each={props.locations}>
                {(location) => (
                  <div class="location-item">
                    <button
                      class="location-item-btn"
                      onClick={() => {
                        props.onJumpTo(location);
                        setIsOpen(false);
                      }}
                    >
                      <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                        <circle cx="5" cy="5" r="3" fill="currentColor"/>
                      </svg>
                      {location.name}
                    </button>
                    <button
                      class="location-delete-btn"
                      onClick={() => props.onDelete(location.id)}
                      title="Delete location"
                    >
                      <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                        <path
                          d="M2 2l6 6M8 2l-6 6"
                          stroke="currentColor"
                          stroke-width="1"
                          stroke-linecap="round"
                        />
                      </svg>
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={!isSaving() && props.locations.length === 0}>
            <div class="location-empty">
              No saved locations
              <br />
              <span class="location-empty-hint">Click "Save" to create one</span>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export default LocationPicker;
