"""
3D Memory Graph Visualizer
===========================

Visualizes the 384-dimensional semantic memory space in 3D.
Uses dimensionality reduction (PCA/t-SNE) to project vectors to 3D,
then renders with OpenGL for interactive exploration.

Features:
- 3D scatter plot of memory entries
- Color-coded by type (document, chunk, image, etc.)
- Interactive rotation, zoom, pan
- Click to select and view entry details
- Relationship edges between linked entries
"""

import numpy as np
from typing import List, Dict, Optional, Tuple
from dataclasses import dataclass
from pathlib import Path

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QComboBox, QSlider, QTextEdit, QSplitter, QGroupBox,
    QCheckBox, QSpinBox, QDialog, QProgressBar, QLineEdit,
    QListWidget, QListWidgetItem
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor

# OpenGL imports
from OpenGL.GL import *
from OpenGL.GLU import *
from PyQt6.QtOpenGLWidgets import QOpenGLWidget


@dataclass
class MemoryPoint:
    """A point in the 3D visualization."""
    id: str
    position: np.ndarray  # 3D position after reduction
    original_vector: np.ndarray  # Original 384D vector
    entry_type: str  # document, chunk, image, etc.
    content: str
    metadata: Dict
    color: Tuple[float, float, float]
    created_at: float = 0.0  # Unix timestamp
    time_normalized: float = 0.0  # 0-1 normalized time (oldest=0, newest=1)


class MemoryGraph3DWidget(QOpenGLWidget):
    """OpenGL widget for 3D memory visualization."""

    point_selected = pyqtSignal(str)  # Emits entry ID when clicked

    # Color scheme for different types
    TYPE_COLORS = {
        'document': (0.2, 0.6, 1.0),      # Blue
        'chunk': (0.4, 0.8, 0.4),          # Green
        'image': (1.0, 0.6, 0.2),          # Orange
        'event': (1.0, 0.3, 0.3),          # Red
        'python_file': (0.3, 0.7, 0.9),   # Light blue
        'javascript_file': (0.9, 0.8, 0.2), # Yellow
        'markdown_file': (0.7, 0.5, 0.9),  # Purple
        'json_file': (0.5, 0.9, 0.7),      # Teal
        'image_file': (1.0, 0.5, 0.7),     # Pink
        'default': (0.7, 0.7, 0.7),        # Gray
    }

    def __init__(self, parent=None):
        super().__init__(parent)
        self.points: List[MemoryPoint] = []
        self.edges: List[Tuple[int, int]] = []  # Index pairs

        # Camera state
        self.rotation_x = 30.0
        self.rotation_y = 45.0
        self.zoom = 5.0
        self.pan_x = 0.0
        self.pan_y = 0.0

        # Interaction state
        self.last_mouse_pos = None
        self.selected_point = None
        self.hovered_point = None

        # Highlighting for search/similar/clusters
        self.highlighted_points: set = set()  # Indices of highlighted points
        self.hidden_points: set = set()  # Indices of filtered-out points
        self.cluster_labels: Optional[np.ndarray] = None  # Cluster assignments

        # Display options
        self.show_edges = True
        self.point_size = 8.0
        self.edge_alpha = 0.3

        # Time dimension options
        self.use_time_z_axis = False  # Use time as Z axis instead of PCA/t-SNE
        self.color_by_time = False  # Color points by time (blue=old, red=new)
        self.time_range = (0.0, 1.0)  # Filter to show only points in this time range
        self.original_positions: Dict[int, np.ndarray] = {}  # Store original 3D positions

        # Animation
        self.auto_rotate = False
        self.rotation_timer = QTimer()
        self.rotation_timer.timeout.connect(self._auto_rotate_tick)

        self.setMinimumSize(400, 400)
        self.setMouseTracking(True)

    def set_points(self, points: List[MemoryPoint]):
        """Set the points to visualize."""
        self.points = points
        self.update()

    def set_edges(self, edges: List[Tuple[int, int]]):
        """Set edges between points."""
        self.edges = edges
        self.update()

    def initializeGL(self):
        """Initialize OpenGL settings."""
        glClearColor(0.1, 0.1, 0.15, 1.0)  # Dark background
        glEnable(GL_DEPTH_TEST)
        glEnable(GL_POINT_SMOOTH)
        glEnable(GL_BLEND)
        glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA)
        glHint(GL_POINT_SMOOTH_HINT, GL_NICEST)

    def resizeGL(self, width, height):
        """Handle window resize."""
        glViewport(0, 0, width, height)
        glMatrixMode(GL_PROJECTION)
        glLoadIdentity()
        aspect = width / height if height > 0 else 1
        gluPerspective(45.0, aspect, 0.1, 100.0)
        glMatrixMode(GL_MODELVIEW)

    def paintGL(self):
        """Render the scene."""
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT)
        glLoadIdentity()

        # Camera transform
        glTranslatef(self.pan_x, self.pan_y, -self.zoom)
        glRotatef(self.rotation_x, 1, 0, 0)
        glRotatef(self.rotation_y, 0, 1, 0)

        # Draw edges first (behind points)
        if self.show_edges and self.edges:
            self._draw_edges()

        # Draw points
        self._draw_points()

        # Draw axes for reference
        self._draw_axes()

    # Cluster colors for up to 20 clusters
    CLUSTER_COLORS = [
        (1.0, 0.3, 0.3), (0.3, 1.0, 0.3), (0.3, 0.3, 1.0),
        (1.0, 1.0, 0.3), (1.0, 0.3, 1.0), (0.3, 1.0, 1.0),
        (1.0, 0.6, 0.3), (0.6, 0.3, 1.0), (0.3, 1.0, 0.6),
        (1.0, 0.3, 0.6), (0.6, 1.0, 0.3), (0.3, 0.6, 1.0),
        (0.8, 0.8, 0.3), (0.8, 0.3, 0.8), (0.3, 0.8, 0.8),
        (0.9, 0.5, 0.5), (0.5, 0.9, 0.5), (0.5, 0.5, 0.9),
        (0.9, 0.9, 0.5), (0.5, 0.9, 0.9),
    ]

    def _get_time_color(self, time_normalized: float) -> Tuple[float, float, float]:
        """Get color based on normalized time (0=old/blue, 1=new/red)."""
        # Gradient from blue (old) through green/yellow to red (new)
        if time_normalized < 0.5:
            # Blue to green
            t = time_normalized * 2
            return (0.2, 0.3 + 0.5 * t, 1.0 - 0.6 * t)
        else:
            # Green to red
            t = (time_normalized - 0.5) * 2
            return (0.2 + 0.8 * t, 0.8 - 0.5 * t, 0.4 - 0.3 * t)

    def _draw_points(self):
        """Draw all memory points."""
        # Draw non-highlighted points first (dimmer)
        glPointSize(self.point_size)
        glBegin(GL_POINTS)

        for i, point in enumerate(self.points):
            # Skip hidden points
            if i in self.hidden_points:
                continue

            # Skip points outside time range
            if not (self.time_range[0] <= point.time_normalized <= self.time_range[1]):
                continue

            # Determine position (use time Z-axis if enabled)
            if self.use_time_z_axis:
                pos = np.array([point.position[0], point.position[1],
                               (point.time_normalized - 0.5) * 2])  # Map to [-1, 1]
            else:
                pos = point.position

            # Determine color
            if i == self.selected_point:
                continue  # Draw selected separately
            elif i in self.highlighted_points:
                continue  # Draw highlighted separately
            elif self.color_by_time:
                # Color by time (blue=old, red=new)
                color = self._get_time_color(point.time_normalized)
                alpha = 0.8
            elif self.cluster_labels is not None and i < len(self.cluster_labels):
                # Use cluster color
                cluster_id = self.cluster_labels[i]
                color = self.CLUSTER_COLORS[cluster_id % len(self.CLUSTER_COLORS)]
                alpha = 0.6
            else:
                color = point.color
                alpha = 0.4 if self.highlighted_points else 0.8

            glColor4f(*color, alpha)
            glVertex3f(*pos)

        glEnd()

        # Draw highlighted points (brighter, larger)
        if self.highlighted_points:
            glPointSize(self.point_size * 1.3)
            glBegin(GL_POINTS)
            for i in self.highlighted_points:
                if i < len(self.points) and i not in self.hidden_points:
                    point = self.points[i]
                    # Skip points outside time range
                    if not (self.time_range[0] <= point.time_normalized <= self.time_range[1]):
                        continue
                    # Use time Z-axis if enabled
                    if self.use_time_z_axis:
                        pos = np.array([point.position[0], point.position[1],
                                       (point.time_normalized - 0.5) * 2])
                    else:
                        pos = point.position
                    glColor4f(1.0, 1.0, 0.3, 1.0)  # Yellow for highlighted
                    glVertex3f(*pos)
            glEnd()

        # Draw selected point (white, largest)
        if self.selected_point is not None and self.selected_point < len(self.points):
            if self.selected_point not in self.hidden_points:
                point = self.points[self.selected_point]
                # Check time range
                if self.time_range[0] <= point.time_normalized <= self.time_range[1]:
                    # Use time Z-axis if enabled
                    if self.use_time_z_axis:
                        pos = np.array([point.position[0], point.position[1],
                                       (point.time_normalized - 0.5) * 2])
                    else:
                        pos = point.position
                    glPointSize(self.point_size * 2)
                    glBegin(GL_POINTS)
                    glColor4f(1.0, 1.0, 1.0, 1.0)
                    glVertex3f(*pos)
                    glEnd()

    def _draw_edges(self):
        """Draw relationship edges."""
        glLineWidth(1.0)
        glBegin(GL_LINES)
        glColor4f(0.5, 0.5, 0.5, self.edge_alpha)

        for i, j in self.edges:
            if i < len(self.points) and j < len(self.points):
                glVertex3f(*self.points[i].position)
                glVertex3f(*self.points[j].position)

        glEnd()

    def _draw_axes(self):
        """Draw coordinate axes for reference."""
        glLineWidth(2.0)
        glBegin(GL_LINES)

        # X axis - Red
        glColor4f(1.0, 0.3, 0.3, 0.5)
        glVertex3f(0, 0, 0)
        glVertex3f(1, 0, 0)

        # Y axis - Green
        glColor4f(0.3, 1.0, 0.3, 0.5)
        glVertex3f(0, 0, 0)
        glVertex3f(0, 1, 0)

        # Z axis - Blue
        glColor4f(0.3, 0.3, 1.0, 0.5)
        glVertex3f(0, 0, 0)
        glVertex3f(0, 0, 1)

        glEnd()

    def mousePressEvent(self, event):
        """Handle mouse press."""
        self.last_mouse_pos = event.position()

        if event.button() == Qt.MouseButton.LeftButton:
            # Try to select a point
            self._pick_point(event.position().x(), event.position().y())

    def mouseMoveEvent(self, event):
        """Handle mouse drag for rotation/pan."""
        if self.last_mouse_pos is None:
            return

        dx = event.position().x() - self.last_mouse_pos.x()
        dy = event.position().y() - self.last_mouse_pos.y()

        if event.buttons() & Qt.MouseButton.LeftButton:
            # Rotate
            self.rotation_y += dx * 0.5
            self.rotation_x += dy * 0.5
        elif event.buttons() & Qt.MouseButton.RightButton:
            # Pan
            self.pan_x += dx * 0.01
            self.pan_y -= dy * 0.01

        self.last_mouse_pos = event.position()
        self.update()

    def mouseReleaseEvent(self, event):
        """Handle mouse release."""
        self.last_mouse_pos = None

    def wheelEvent(self, event):
        """Handle mouse wheel for zoom, centered on mouse position."""
        delta = event.angleDelta().y()

        # Get mouse position relative to center
        pos = event.position()
        center_x = self.width() / 2
        center_y = self.height() / 2

        # Calculate offset from center (normalized)
        offset_x = (pos.x() - center_x) / self.width()
        offset_y = (center_y - pos.y()) / self.height()  # Flip Y

        # Store old zoom
        old_zoom = self.zoom

        # Apply zoom
        self.zoom -= delta * 0.005
        self.zoom = max(1.0, min(50.0, self.zoom))

        # Adjust pan to keep mouse position fixed
        zoom_ratio = self.zoom / old_zoom if old_zoom > 0 else 1
        self.pan_x += offset_x * (1 - zoom_ratio) * 0.5
        self.pan_y += offset_y * (1 - zoom_ratio) * 0.5

        self.update()

    def _pick_point(self, x, y):
        """Try to pick a point at screen coordinates."""
        # Simple nearest-point picking based on projected coordinates
        # For more accurate picking, would need to unproject or use selection buffer

        if not self.points:
            return

        # Get modelview and projection matrices
        self.makeCurrent()
        modelview = glGetDoublev(GL_MODELVIEW_MATRIX)
        projection = glGetDoublev(GL_PROJECTION_MATRIX)
        viewport = glGetIntegerv(GL_VIEWPORT)

        # Find closest point
        closest_dist = float('inf')
        closest_idx = None

        for i, point in enumerate(self.points):
            # Project 3D point to screen
            try:
                screen = gluProject(
                    point.position[0], point.position[1], point.position[2],
                    modelview, projection, viewport
                )
                if screen:
                    sx, sy, _ = screen
                    sy = viewport[3] - sy  # Flip Y
                    dist = (sx - x) ** 2 + (sy - y) ** 2
                    if dist < closest_dist and dist < 400:  # Within 20 pixels
                        closest_dist = dist
                        closest_idx = i
            except:
                pass

        if closest_idx is not None:
            self.selected_point = closest_idx
            self.point_selected.emit(self.points[closest_idx].id)
        else:
            self.selected_point = None

        self.update()

    def toggle_auto_rotate(self, enabled: bool):
        """Toggle auto-rotation."""
        self.auto_rotate = enabled
        if enabled:
            self.rotation_timer.start(30)  # ~33 FPS
        else:
            self.rotation_timer.stop()

    def _auto_rotate_tick(self):
        """Auto-rotation animation tick."""
        self.rotation_y += 0.5
        self.update()

    def reset_view(self):
        """Reset camera to default view."""
        self.rotation_x = 30.0
        self.rotation_y = 45.0
        self.zoom = 5.0
        self.pan_x = 0.0
        self.pan_y = 0.0
        self.update()


class MemoryVisualizerDialog(QDialog):
    """Full dialog for memory visualization with controls."""

    def __init__(self, kernel, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.points: List[MemoryPoint] = []
        self.id_to_index: Dict[str, int] = {}

        self.setWindowTitle("3D Semantic Memory Visualizer")
        self.setMinimumSize(1000, 700)
        self.setup_ui()

    def setup_ui(self):
        """Setup the UI."""
        layout = QHBoxLayout(self)

        # Main splitter
        splitter = QSplitter(Qt.Orientation.Horizontal)

        # Left side - 3D view
        left_widget = QWidget()
        left_layout = QVBoxLayout(left_widget)

        self.gl_widget = MemoryGraph3DWidget()
        self.gl_widget.point_selected.connect(self._on_point_selected)
        left_layout.addWidget(self.gl_widget)

        # Controls below 3D view
        controls = QHBoxLayout()

        self.load_btn = QPushButton("Load Memory")
        self.load_btn.clicked.connect(self.load_memory)
        controls.addWidget(self.load_btn)

        self.reset_btn = QPushButton("Reset View")
        self.reset_btn.clicked.connect(self.gl_widget.reset_view)
        controls.addWidget(self.reset_btn)

        self.rotate_cb = QCheckBox("Auto-rotate")
        self.rotate_cb.toggled.connect(self.gl_widget.toggle_auto_rotate)
        controls.addWidget(self.rotate_cb)

        self.edges_cb = QCheckBox("Show Edges")
        self.edges_cb.setChecked(True)
        self.edges_cb.toggled.connect(self._toggle_edges)
        controls.addWidget(self.edges_cb)

        controls.addStretch()

        # Reduction method
        controls.addWidget(QLabel("Method:"))
        self.method_combo = QComboBox()
        self.method_combo.addItems(["PCA (Fast)", "t-SNE (Slow but clusters)"])
        controls.addWidget(self.method_combo)

        # Point size
        controls.addWidget(QLabel("Size:"))
        self.size_slider = QSlider(Qt.Orientation.Horizontal)
        self.size_slider.setRange(2, 20)
        self.size_slider.setValue(8)
        self.size_slider.setMaximumWidth(100)
        self.size_slider.valueChanged.connect(self._update_point_size)
        controls.addWidget(self.size_slider)

        left_layout.addLayout(controls)

        # Second row of controls - Search and Filter
        controls2 = QHBoxLayout()

        # Search
        controls2.addWidget(QLabel("Search:"))
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search content...")
        self.search_input.setMaximumWidth(200)
        self.search_input.returnPressed.connect(self._search_points)
        controls2.addWidget(self.search_input)

        search_btn = QPushButton("Find")
        search_btn.clicked.connect(self._search_points)
        controls2.addWidget(search_btn)

        clear_search_btn = QPushButton("Clear")
        clear_search_btn.clicked.connect(self._clear_search)
        controls2.addWidget(clear_search_btn)

        controls2.addStretch()

        # Cluster detection
        controls2.addWidget(QLabel("Clusters:"))
        self.cluster_spin = QSpinBox()
        self.cluster_spin.setRange(2, 20)
        self.cluster_spin.setValue(5)
        self.cluster_spin.setMaximumWidth(60)
        controls2.addWidget(self.cluster_spin)

        cluster_btn = QPushButton("Detect Clusters")
        cluster_btn.clicked.connect(self._detect_clusters)
        controls2.addWidget(cluster_btn)

        left_layout.addLayout(controls2)

        # Third row of controls - Time dimension
        controls3 = QHBoxLayout()

        controls3.addWidget(QLabel("Time:"))

        self.time_z_cb = QCheckBox("Z-axis = Time")
        self.time_z_cb.toggled.connect(self._toggle_time_z)
        controls3.addWidget(self.time_z_cb)

        self.color_time_cb = QCheckBox("Color by Time")
        self.color_time_cb.toggled.connect(self._toggle_color_time)
        controls3.addWidget(self.color_time_cb)

        controls3.addWidget(QLabel("Range:"))

        self.time_min_slider = QSlider(Qt.Orientation.Horizontal)
        self.time_min_slider.setRange(0, 100)
        self.time_min_slider.setValue(0)
        self.time_min_slider.setMaximumWidth(80)
        self.time_min_slider.valueChanged.connect(self._update_time_range)
        controls3.addWidget(self.time_min_slider)

        self.time_range_label = QLabel("0% - 100%")
        self.time_range_label.setMinimumWidth(80)
        controls3.addWidget(self.time_range_label)

        self.time_max_slider = QSlider(Qt.Orientation.Horizontal)
        self.time_max_slider.setRange(0, 100)
        self.time_max_slider.setValue(100)
        self.time_max_slider.setMaximumWidth(80)
        self.time_max_slider.valueChanged.connect(self._update_time_range)
        controls3.addWidget(self.time_max_slider)

        controls3.addStretch()

        left_layout.addLayout(controls3)

        # Progress bar
        self.progress = QProgressBar()
        self.progress.setVisible(False)
        left_layout.addWidget(self.progress)

        splitter.addWidget(left_widget)

        # Right side - Details panel
        right_widget = QWidget()
        right_layout = QVBoxLayout(right_widget)

        # Search results / Similar points
        similar_group = QGroupBox("Similar Points / Search Results")
        similar_layout = QVBoxLayout(similar_group)

        self.find_similar_btn = QPushButton("Find Similar to Selected")
        self.find_similar_btn.clicked.connect(self._find_similar)
        self.find_similar_btn.setEnabled(False)
        similar_layout.addWidget(self.find_similar_btn)

        self.similar_list = QListWidget()
        self.similar_list.setMaximumHeight(120)
        self.similar_list.itemClicked.connect(self._on_similar_clicked)
        similar_layout.addWidget(self.similar_list)
        right_layout.addWidget(similar_group)

        # Type filters
        filter_group = QGroupBox("Filter by Type")
        filter_layout = QVBoxLayout(filter_group)

        self.type_checkboxes = {}
        for type_name in MemoryGraph3DWidget.TYPE_COLORS.keys():
            if type_name != 'default':
                cb = QCheckBox(type_name)
                cb.setChecked(True)
                cb.toggled.connect(self._apply_filters)
                self.type_checkboxes[type_name] = cb
                filter_layout.addWidget(cb)

        filter_buttons = QHBoxLayout()
        select_all_btn = QPushButton("All")
        select_all_btn.clicked.connect(lambda: self._set_all_filters(True))
        filter_buttons.addWidget(select_all_btn)
        select_none_btn = QPushButton("None")
        select_none_btn.clicked.connect(lambda: self._set_all_filters(False))
        filter_buttons.addWidget(select_none_btn)
        filter_layout.addLayout(filter_buttons)

        right_layout.addWidget(filter_group)

        # Stats
        stats_group = QGroupBox("Statistics")
        stats_layout = QVBoxLayout(stats_group)
        self.stats_label = QLabel("No data loaded")
        stats_layout.addWidget(self.stats_label)
        right_layout.addWidget(stats_group)

        # Cluster info
        cluster_group = QGroupBox("Cluster Info")
        cluster_layout = QVBoxLayout(cluster_group)
        self.cluster_label = QLabel("No clusters detected")
        self.cluster_label.setWordWrap(True)
        cluster_layout.addWidget(self.cluster_label)
        right_layout.addWidget(cluster_group)

        # Selected entry details
        details_group = QGroupBox("Selected Entry")
        details_layout = QVBoxLayout(details_group)
        self.details_text = QTextEdit()
        self.details_text.setReadOnly(True)
        self.details_text.setPlaceholderText("Click a point to see details")
        details_layout.addWidget(self.details_text)
        right_layout.addWidget(details_group)

        splitter.addWidget(right_widget)
        splitter.setSizes([700, 300])

        layout.addWidget(splitter)

    def load_memory(self):
        """Load memory entries and reduce to 3D."""
        self.progress.setVisible(True)
        self.progress.setValue(0)
        self.load_btn.setEnabled(False)

        try:
            # Load vectors from database
            self.progress.setValue(10)
            entries = self._load_entries()

            if not entries:
                self.stats_label.setText("No entries with vectors found")
                return

            self.progress.setValue(30)

            # Extract vectors
            vectors = np.array([e['vector'] for e in entries])

            # Reduce dimensions
            self.progress.setValue(50)
            method = self.method_combo.currentText()

            if "PCA" in method:
                from sklearn.decomposition import PCA
                reducer = PCA(n_components=3)
                positions = reducer.fit_transform(vectors)
            else:
                from sklearn.manifold import TSNE
                reducer = TSNE(n_components=3, perplexity=min(30, len(vectors)-1), random_state=42)
                positions = reducer.fit_transform(vectors)

            self.progress.setValue(80)

            # Normalize positions to [-1, 1] range
            positions = positions - positions.mean(axis=0)
            max_range = np.abs(positions).max()
            if max_range > 0:
                positions = positions / max_range

            # Calculate time range for normalization
            timestamps = [e.get('created_at', 0) or 0 for e in entries]
            time_min = min(timestamps) if timestamps else 0
            time_max = max(timestamps) if timestamps else 1
            time_range = time_max - time_min if time_max > time_min else 1

            # Create points
            self.points = []
            self.id_to_index = {}

            for i, entry in enumerate(entries):
                # Use metadata type if available (more specific), else column type
                metadata = entry.get('metadata', {})
                entry_type = metadata.get('type', entry.get('type', 'default'))

                color = MemoryGraph3DWidget.TYPE_COLORS.get(
                    entry_type,
                    MemoryGraph3DWidget.TYPE_COLORS['default']
                )

                # Calculate normalized time (0=oldest, 1=newest)
                created_at = entry.get('created_at', 0) or 0
                time_normalized = (created_at - time_min) / time_range if time_range > 0 else 0.5

                point = MemoryPoint(
                    id=entry['id'],
                    position=positions[i],
                    original_vector=vectors[i],
                    entry_type=entry_type,
                    content=entry.get('content', '')[:500],
                    metadata=metadata,
                    color=color,
                    created_at=created_at,
                    time_normalized=time_normalized
                )
                self.points.append(point)
                self.id_to_index[entry['id']] = i

            # Load edges (relations)
            edges = self._load_relations()

            self.progress.setValue(90)

            # Update visualization
            self.gl_widget.set_points(self.points)
            self.gl_widget.set_edges(edges)

            # Update stats
            type_counts = {}
            for p in self.points:
                type_counts[p.entry_type] = type_counts.get(p.entry_type, 0) + 1

            stats_text = f"Total entries: {len(self.points)}\n"
            stats_text += f"Relations: {len(edges)}\n\n"

            # Time range info
            if time_min > 0 and time_max > 0:
                from datetime import datetime
                dt_min = datetime.fromtimestamp(time_min)
                dt_max = datetime.fromtimestamp(time_max)
                stats_text += f"Time range:\n"
                stats_text += f"  Oldest: {dt_min.strftime('%Y-%m-%d')}\n"
                stats_text += f"  Newest: {dt_max.strftime('%Y-%m-%d')}\n\n"

            stats_text += "By type:\n"
            for t, c in sorted(type_counts.items(), key=lambda x: -x[1]):
                stats_text += f"  {t}: {c}\n"

            self.stats_label.setText(stats_text)

            self.progress.setValue(100)

        except Exception as e:
            self.stats_label.setText(f"Error: {str(e)}")
            import traceback
            traceback.print_exc()

        finally:
            self.progress.setVisible(False)
            self.load_btn.setEnabled(True)

    def _load_entries(self) -> List[Dict]:
        """Load memory entries with vectors from database."""
        import sqlite3
        import json

        db_path = self.kernel.memory.db_path
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()

        cursor.execute('''
            SELECT id, type, content, metadata, vector, created_at
            FROM memory
            WHERE vector IS NOT NULL
            LIMIT 5000
        ''')

        entries = []
        for row in cursor.fetchall():
            id_, type_, content, metadata_str, vector_blob, created_at = row

            if vector_blob:
                # Decode vector from blob
                vector = np.frombuffer(vector_blob, dtype=np.float32)

                entries.append({
                    'id': id_,
                    'type': type_,
                    'content': content,
                    'metadata': json.loads(metadata_str) if metadata_str else {},
                    'vector': vector,
                    'created_at': created_at or 0.0
                })

        conn.close()
        return entries

    def _load_relations(self) -> List[Tuple[int, int]]:
        """Load relations as edge indices."""
        import sqlite3

        db_path = self.kernel.memory.db_path
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()

        # Relations use source_path and target_path, need to map to memory IDs
        cursor.execute('SELECT source_path, target_path FROM relations')

        edges = []
        for source_path, target_path in cursor.fetchall():
            # Find memory entries with matching paths
            source_idx = None
            target_idx = None
            for i, point in enumerate(self.points):
                path = point.metadata.get('path', '')
                if path == source_path:
                    source_idx = i
                if path == target_path:
                    target_idx = i
            if source_idx is not None and target_idx is not None:
                edges.append((source_idx, target_idx))

        conn.close()
        return edges

    def _on_point_selected(self, entry_id: str):
        """Handle point selection."""
        if entry_id not in self.id_to_index:
            self.find_similar_btn.setEnabled(False)
            return

        self.find_similar_btn.setEnabled(True)
        point = self.points[self.id_to_index[entry_id]]

        details = f"ID: {point.id[:50]}...\n\n"
        details += f"Type: {point.entry_type}\n\n"

        # Show time info
        if point.created_at > 0:
            from datetime import datetime
            dt = datetime.fromtimestamp(point.created_at)
            details += f"Created: {dt.strftime('%Y-%m-%d %H:%M:%S')}\n"
            details += f"Time position: {point.time_normalized:.1%} (oldest to newest)\n\n"

        details += f"Content:\n{point.content[:1000]}\n\n"
        details += f"Metadata:\n"

        for key, value in list(point.metadata.items())[:10]:
            if key not in ('vector', 'content'):
                details += f"  {key}: {str(value)[:100]}\n"

        self.details_text.setText(details)

    def _toggle_edges(self, show: bool):
        """Toggle edge visibility."""
        self.gl_widget.show_edges = show
        self.gl_widget.update()

    def _update_point_size(self, size: int):
        """Update point size."""
        self.gl_widget.point_size = float(size)
        self.gl_widget.update()

    def _toggle_time_z(self, enabled: bool):
        """Toggle using time as Z axis."""
        self.gl_widget.use_time_z_axis = enabled
        self.gl_widget.update()

    def _toggle_color_time(self, enabled: bool):
        """Toggle coloring by time."""
        self.gl_widget.color_by_time = enabled
        # Clear cluster coloring when enabling time coloring
        if enabled:
            self.gl_widget.cluster_labels = None
        self.gl_widget.update()

    def _update_time_range(self):
        """Update time range filter."""
        min_val = self.time_min_slider.value() / 100.0
        max_val = self.time_max_slider.value() / 100.0

        # Ensure min <= max
        if min_val > max_val:
            if self.sender() == self.time_min_slider:
                self.time_max_slider.setValue(int(min_val * 100))
                max_val = min_val
            else:
                self.time_min_slider.setValue(int(max_val * 100))
                min_val = max_val

        self.gl_widget.time_range = (min_val, max_val)
        self.time_range_label.setText(f"{int(min_val*100)}% - {int(max_val*100)}%")
        self.gl_widget.update()

    def _search_points(self):
        """Search for points containing text."""
        query = self.search_input.text().strip().lower()
        if not query or not self.points:
            return

        self.similar_list.clear()
        self.gl_widget.highlighted_points.clear()

        matches = []
        for i, point in enumerate(self.points):
            # Search in content and metadata
            content = point.content.lower()
            path = point.metadata.get('path', '').lower()
            name = Path(path).name.lower() if path else ''

            if query in content or query in path or query in name:
                matches.append((i, point))
                self.gl_widget.highlighted_points.add(i)

        # Show results in list
        for i, point in matches[:50]:  # Limit to 50 results
            name = Path(point.metadata.get('path', '')).name or point.id[:30]
            item = QListWidgetItem(f"{name} ({point.entry_type})")
            item.setData(Qt.ItemDataRole.UserRole, i)
            self.similar_list.addItem(item)

        self.gl_widget.update()
        self.stats_label.setText(f"Found {len(matches)} matches for '{query}'")

    def _clear_search(self):
        """Clear search highlighting."""
        self.search_input.clear()
        self.similar_list.clear()
        self.gl_widget.highlighted_points.clear()
        self.gl_widget.update()

    def _detect_clusters(self):
        """Detect clusters using KMeans."""
        if not self.points:
            return

        try:
            from sklearn.cluster import KMeans

            n_clusters = self.cluster_spin.value()

            # Use original vectors for clustering (better than 3D positions)
            vectors = np.array([p.original_vector for p in self.points])

            kmeans = KMeans(n_clusters=n_clusters, random_state=42, n_init=10)
            labels = kmeans.fit_predict(vectors)

            self.gl_widget.cluster_labels = labels

            # Analyze clusters
            cluster_info = []
            for c in range(n_clusters):
                indices = np.where(labels == c)[0]
                types = [self.points[i].entry_type for i in indices]
                type_counts = {}
                for t in types:
                    type_counts[t] = type_counts.get(t, 0) + 1
                dominant_type = max(type_counts, key=type_counts.get) if type_counts else 'mixed'
                cluster_info.append(f"Cluster {c+1}: {len(indices)} points ({dominant_type})")

            self.cluster_label.setText('\n'.join(cluster_info))
            self.gl_widget.update()

        except Exception as e:
            self.cluster_label.setText(f"Error: {str(e)}")

    def _find_similar(self):
        """Find points similar to selected point."""
        if self.gl_widget.selected_point is None:
            return

        selected_idx = self.gl_widget.selected_point
        if selected_idx >= len(self.points):
            return

        selected_point = self.points[selected_idx]
        selected_vector = selected_point.original_vector

        # Calculate cosine similarity to all other points
        similarities = []
        for i, point in enumerate(self.points):
            if i == selected_idx:
                continue
            # Cosine similarity
            dot = np.dot(selected_vector, point.original_vector)
            norm = np.linalg.norm(selected_vector) * np.linalg.norm(point.original_vector)
            sim = dot / norm if norm > 0 else 0
            similarities.append((i, sim, point))

        # Sort by similarity
        similarities.sort(key=lambda x: -x[1])

        # Show top 10 in list and highlight
        self.similar_list.clear()
        self.gl_widget.highlighted_points.clear()

        for i, sim, point in similarities[:10]:
            name = Path(point.metadata.get('path', '')).name or point.id[:30]
            item = QListWidgetItem(f"{sim:.3f} - {name}")
            item.setData(Qt.ItemDataRole.UserRole, i)
            self.similar_list.addItem(item)
            self.gl_widget.highlighted_points.add(i)

        self.gl_widget.update()

    def _on_similar_clicked(self, item):
        """Handle click on similar/search result item."""
        idx = item.data(Qt.ItemDataRole.UserRole)
        if idx is not None and idx < len(self.points):
            self.gl_widget.selected_point = idx
            self.gl_widget.point_selected.emit(self.points[idx].id)
            self.gl_widget.update()

    def _apply_filters(self):
        """Apply type filters to hide/show points."""
        if not self.points:
            return

        self.gl_widget.hidden_points.clear()

        for i, point in enumerate(self.points):
            entry_type = point.entry_type
            # Check if this type is unchecked
            if entry_type in self.type_checkboxes:
                if not self.type_checkboxes[entry_type].isChecked():
                    self.gl_widget.hidden_points.add(i)
            elif 'default' in self.type_checkboxes:
                if not self.type_checkboxes['default'].isChecked():
                    self.gl_widget.hidden_points.add(i)

        visible = len(self.points) - len(self.gl_widget.hidden_points)
        self.stats_label.setText(f"Showing {visible} of {len(self.points)} points")
        self.gl_widget.update()

    def _set_all_filters(self, checked: bool):
        """Set all type filters to checked/unchecked."""
        for cb in self.type_checkboxes.values():
            cb.setChecked(checked)


def show_memory_visualizer(kernel, parent=None):
    """Show the memory visualizer dialog."""
    dialog = MemoryVisualizerDialog(kernel, parent)
    dialog.show()
    return dialog
