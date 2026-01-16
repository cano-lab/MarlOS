"""
File Relationship Explorer
===========================

Visualizes semantic relationships between files using an interactive graph.

Pro Feature: Shows how files connect through relationships like:
- implements, extends, requires
- references, imports
- part_of, derived_from
"""

import json
from pathlib import Path
from typing import Dict, List, Set, Tuple
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QGraphicsView,
    QGraphicsScene, QGraphicsEllipseItem, QGraphicsLineItem,
    QGraphicsTextItem, QToolBar, QComboBox, QPushButton,
    QLabel, QSlider, QLineEdit, QFormLayout, QDialog,
    QGraphicsItem, QDockWidget
)
from PyQt6.QtCore import Qt, QRectF, QPointF, QTimer, pyqtSignal
from PyQt6.QtGui import (
    QPainter, QPen, QBrush, QColor, QFont,
    QWheelEvent, QMouseEvent, QPainterPath
)
import math
import random


class GraphNode(QGraphicsEllipseItem):
    """A node in the relationship graph."""

    def __init__(self, file_path: str, node_id: str, parent=None):
        super().__init__(-30, -30, 60, 60, parent)
        self.file_path = file_path
        self.node_id = node_id
        self.file_name = Path(file_path).name
        self.edges: List['GraphEdge'] = []

        # Visual properties
        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsMovable)
        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemIsSelectable)
        self.setFlag(QGraphicsItem.GraphicsItemFlag.ItemSendsGeometryChanges)

        # Color by file type
        self.color = self._get_color_by_type()
        self.setBrush(QBrush(self.color))

        # Border
        pen = QPen(Qt.PenStyle.SolidLine)
        pen.setWidth(2)
        pen.setColor(QColor(50, 50, 50))
        self.setPen(pen)

        # Tooltip
        self.setToolTip(f"{self.file_path}")

    def _get_color_by_type(self) -> QColor:
        """Get color based on file type."""
        ext = Path(self.file_path).suffix.lower()

        # Python files
        if ext == '.py':
            return QColor(100, 150, 255)  # Blue
        # Markdown
        elif ext in ['.md', '.markdown']:
            return QColor(255, 200, 100)  # Orange
        # JavaScript/TypeScript
        elif ext in ['.js', '.ts', '.jsx', '.tsx']:
            return QColor(255, 220, 100)  # Yellow
        # CSS/HTML
        elif ext in ['.css', '.html', '.htm']:
            return QColor(150, 100, 255)  # Purple
        # JSON/YAML
        elif ext in ['.json', '.yaml', '.yml']:
            return QColor(150, 255, 150)  # Green
        # Text
        elif ext in ['.txt', '.text']:
            return QColor(200, 200, 200)  # Gray
        # Default
        else:
            return QColor(255, 255, 255)  # White

    def add_edge(self, edge: 'GraphEdge'):
        """Add an edge connected to this node."""
        self.edges.append(edge)

    def itemChange(self, change, value):
        """Handle item changes (position updates)."""
        if change == QGraphicsItem.GraphicsItemChange.ItemPositionChange:
            # Update connected edges
            for edge in self.edges:
                edge.update_position()
        return super().itemChange(change, value)

    def paint(self, painter: QPainter, option: QWidget, widget=None):
        """Custom paint to show file name."""
        super().paint(painter, option, widget)

        # Draw file name below node
        painter.setPen(QColor(0, 0, 0))
        font = QFont()
        font.setPointSize(8)
        painter.setFont(font)

        # Truncate long names
        display_name = self.file_name
        if len(display_name) > 15:
            display_name = display_name[:12] + "..."

        # Center text
        rect = self.boundingRect()
        painter.drawText(
            int(rect.x()),
            int(rect.y() + rect.height() + 15),
            int(rect.width()),
            20,
            Qt.AlignmentFlag.AlignCenter,
            display_name
        )


class GraphEdge(QGraphicsLineItem):
    """An edge in the relationship graph."""

    # Color scheme by relation type
    RELATION_COLORS = {
        "implements": QColor(100, 150, 255),      # Blue
        "extends": QColor(150, 100, 255),         # Purple
        "requires": QColor(255, 100, 100),        # Red
        "references": QColor(255, 200, 100),      # Orange
        "derived_from": QColor(100, 255, 100),    # Green
        "part_of": QColor(255, 150, 200),         # Pink
        "related_to": QColor(200, 200, 200),      # Gray
    }

    def __init__(self, source_node: GraphNode, target_node: GraphNode,
                 relation: str, label: str = None, parent=None):
        super().__init__(parent)
        self.source_node = source_node
        self.target_node = target_node
        self.relation = relation
        self.label = label or relation

        # Set color by relation type
        color = self.RELATION_COLORS.get(relation, QColor(150, 150, 150))
        self.setPen(QPen(color, 2))

        # Add edge to nodes
        source_node.add_edge(self)
        target_node.add_edge(self)

        # Tooltip
        self.setToolTip(f"{self.source_node.file_name}\n{self.relation}\n{self.target_node.file_name}")

        self.update_position()

    def update_position(self):
        """Update line position based on node positions."""
        line = self.line()
        line.setP1(self.source_node.pos())
        line.setP2(self.target_node.pos())
        self.setLine(line)

    def paint(self, painter: QPainter, option: QWidget, widget=None):
        """Custom paint with arrow."""
        super().paint(painter, option, widget)

        # Draw arrow at midpoint
        line = self.line()
        mid_point = (line.p1() + line.p2()) / 2

        # Arrow direction
        dx = line.dx()
        dy = line.dy()
        angle = math.atan2(dy, dx)

        # Draw small circle at midpoint to show direction
        painter.setBrush(self.pen().color())
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawEllipse(mid_point, 4, 4)


class RelationGraphScene(QGraphicsScene):
    """Scene for the relationship graph."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setSceneRect(-2000, -2000, 4000, 4000)
        self.nodes: Dict[str, GraphNode] = {}
        self.edges: List[GraphEdge] = []


class RelationGraphView(QGraphicsView):
    """Interactive view of the relationship graph."""

    node_selected = pyqtSignal(str)  # file_path

    def __init__(self, scene: RelationGraphScene, parent=None):
        super().__init__(scene, parent)
        self.setRenderHint(QPainter.RenderHint.Antialiasing)
        self.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)
        self.setViewportUpdateMode(QGraphicsView.ViewportUpdateMode.FullViewportUpdate)

        # Zoom
        self.zoom_level = 1.0

    def wheelEvent(self, event: QWheelEvent):
        """Handle mouse wheel for zooming."""
        zoom_in_factor = 1.15
        zoom_out_factor = 1 / zoom_in_factor

        if event.angleDelta().y() > 0:
            zoom_factor = zoom_in_factor
        else:
            zoom_factor = zoom_out_factor

        self.scale(zoom_factor, zoom_factor)
        self.zoom_level *= zoom_factor


class RelationExplorer(QWidget):
    """File relationship explorer widget."""

    def __init__(self, kernel, parent=None):
        super().__init__(parent)
        self.kernel = kernel
        self.scene = RelationGraphScene()
        self.view = RelationGraphView(self.scene)

        # Current filter
        self.current_relation_filter = "All"
        self.min_weight = 0

        self.setup_ui()
        self.load_graph()

    def setup_ui(self):
        """Setup the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Toolbar
        toolbar = QHBoxLayout()

        # Relation filter
        toolbar.addWidget(QLabel("Filter:"))
        self.relation_filter = QComboBox()
        self.relation_filter.addItems([
            "All",
            "implements",
            "extends",
            "requires",
            "references",
            "derived_from",
            "part_of"
        ])
        self.relation_filter.currentTextChanged.connect(self.on_filter_changed)
        toolbar.addWidget(self.relation_filter)

        toolbar.addWidget(QLabel("  |  "))

        # Refresh button
        refresh_btn = QPushButton("Refresh")
        refresh_btn.clicked.connect(self.load_graph)
        toolbar.addWidget(refresh_btn)

        # Layout button
        layout_btn = QPushButton("Auto Layout")
        layout_btn.clicked.connect(self.auto_layout)
        toolbar.addWidget(layout_btn)

        toolbar.addWidget(QLabel("  |  "))

        # Stats label
        self.stats_label = QLabel("Nodes: 0 | Edges: 0")
        toolbar.addWidget(self.stats_label)

        toolbar.addStretch()
        layout.addLayout(toolbar)

        # Graph view
        layout.addWidget(self.view)

    def load_graph(self):
        """Load the relationship graph from kernel."""
        # Clear existing
        self.scene.clear()
        self.scene.nodes = {}
        self.scene.edges = []

        # Get all relations from kernel
        relations = self.kernel.relations.get_graph_summary()
        all_relations = self.kernel.relations.find_by_relation("") if relations.get("edges", 0) > 0 else []

        # Track nodes and edges
        node_positions: Dict[str, Tuple[float, float]] = {}

        # Add nodes and edges
        for rel in all_relations:
            source = rel["source"]
            target = rel["target"]
            relation = rel["relation"]
            label = rel.get("label")

            # Create nodes if not exists
            if source not in self.scene.nodes:
                node = GraphNode(source, source)
                self.scene.addItem(node)
                self.scene.nodes[source] = node

                # Initial position (circular layout)
                if source not in node_positions:
                    angle = len(node_positions) * 2 * math.pi / 30
                    x = 400 * math.cos(angle)
                    y = 400 * math.sin(angle)
                    node.setPos(x, y)
                    node_positions[source] = (x, y)

            if target not in self.scene.nodes:
                node = GraphNode(target, target)
                self.scene.addItem(node)
                self.scene.nodes[target] = node

                # Initial position
                if target not in node_positions:
                    angle = len(node_positions) * 2 * math.pi / 30
                    x = 400 * math.cos(angle)
                    y = 400 * math.sin(angle)
                    node.setPos(x, y)
                    node_positions[target] = (x, y)

            # Apply filter
            if self.current_relation_filter != "All" and relation != self.current_relation_filter:
                continue

            # Create edge
            edge = GraphEdge(
                self.scene.nodes[source],
                self.scene.nodes[target],
                relation,
                label
            )
            self.scene.addItem(edge)
            self.scene.edges.append(edge)

        # Update stats
        visible_nodes = len(self.scene.nodes)
        visible_edges = len([e for e in self.scene.edges if e.isVisible()])
        self.stats_label.setText(f"Nodes: {visible_nodes} | Edges: {visible_edges}")

        # Apply force-directed layout
        QTimer.singleShot(100, self.auto_layout)

    def on_filter_changed(self, filter_text: str):
        """Handle filter change."""
        self.current_relation_filter = filter_text

        # Show/hide edges based on filter
        for edge in self.scene.edges:
            if filter_text == "All" or edge.relation == filter_text:
                edge.setVisible(True)
            else:
                edge.setVisible(False)

        # Update edge count
        visible_edges = len([e for e in self.scene.edges if e.isVisible()])
        self.stats_label.setText(f"Nodes: {len(self.scene.nodes)} | Edges: {visible_edges}")

    def auto_layout(self):
        """Apply force-directed layout algorithm."""
        if len(self.scene.nodes) < 2:
            return

        # Simple force-directed layout
        iterations = 50
        k = 100  # Optimal distance
        width = 800
        height = 600

        for iteration in range(iterations):
            # Calculate forces
            forces = {}

            # Repulsion between all nodes
            for node1_id, node1 in self.scene.nodes.items():
                fx, fy = 0, 0

                for node2_id, node2 in self.scene.nodes.items():
                    if node1_id == node2_id:
                        continue

                    dx = node1.x() - node2.x()
                    dy = node1.y() - node2.y()
                    dist = math.sqrt(dx * dx + dy * dy) or 1

                    # Repulsion force
                    force = (k * k) / dist
                    fx += (dx / dist) * force
                    fy += (dy / dist) * force

                forces[node1_id] = (fx, fy)

            # Attraction along edges
            for edge in self.scene.edges:
                if not edge.isVisible():
                    continue

                source = edge.source_node
                target = edge.target_node

                dx = target.x() - source.x()
                dy = target.y() - source.y()
                dist = math.sqrt(dx * dx + dy * dy) or 1

                # Attraction force
                force = (dist * dist) / k
                fx = (dx / dist) * force
                fy = (dy / dist) * force

                # Apply to both nodes
                sfx, sfy = forces[source.node_id]
                forces[source.node_id] = (sfx + fx, sfy + fy)

                tfx, tfy = forces[target.node_id]
                forces[target.node_id] = (tfx - fx, tfy - fy)

            # Apply forces with cooling
            cooling = max(0.1, 1 - iteration / iterations)
            temperature = 10 * cooling

            for node_id, (fx, fy) in forces.items():
                node = self.scene.nodes[node_id]

                # Limit force
                force_mag = math.sqrt(fx * fx + fy * fy) or 1
                if force_mag > temperature:
                    fx = (fx / force_mag) * temperature
                    fy = (fy / force_mag) * temperature

                # Move node
                new_x = node.x() + fx
                new_y = node.y() + fy

                # Keep in bounds
                new_x = max(-width/2, min(width/2, new_x))
                new_y = max(-height/2, min(height/2, new_y))

                node.setPos(new_x, new_y)

        # Center view on graph
        self.view.centerOn(0, 0)


def create_relation_explorer_dock(kernel, parent=None) -> QDockWidget:
    """Create a dockable relation explorer widget."""
    explorer = RelationExplorer(kernel, parent)

    dock = QDockWidget("File Relationships", parent)
    dock.setWidget(explorer)
    dock.setFeatures(
        QDockWidget.DockWidgetFeature.DockWidgetMovable |
        QDockWidget.DockWidgetFeature.DockWidgetFloatable |
        QDockWidget.DockWidgetFeature.DockWidgetClosable
    )

    return dock
