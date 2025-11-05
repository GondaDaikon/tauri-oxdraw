use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use oxdraw::*;

// Application state management
pub struct AppState {
    source_path: Mutex<Option<PathBuf>>,
    background: Mutex<String>,
    overrides: Mutex<LayoutOverrides>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            source_path: Mutex::new(None),
            background: Mutex::new("white".to_string()),
            overrides: Mutex::new(LayoutOverrides::default()),
        }
    }
}

// Payload structures for API responses (matching serve.rs)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagramPayload {
    source_path: String,
    background: String,
    auto_size: CanvasSize,
    render_size: CanvasSize,
    nodes: Vec<NodePayload>,
    edges: Vec<EdgePayload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subgraphs: Vec<SubgraphPayload>,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePayload {
    id: String,
    label: String,
    shape: String,
    auto_position: Point,
    rendered_position: Point,
    #[serde(skip_serializing_if = "Option::is_none")]
    override_position: Option<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fill_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stroke_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    membership: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubgraphPayload {
    id: String,
    label: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    label_x: f32,
    label_y: f32,
    depth: usize,
    order: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgePayload {
    id: String,
    from: String,
    to: String,
    label: Option<String>,
    kind: String,
    auto_points: Vec<Point>,
    rendered_points: Vec<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    override_points: Option<Vec<Point>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arrow_direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_style: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LayoutUpdate {
    #[serde(default)]
    nodes: HashMap<String, Option<Point>>,
    #[serde(default)]
    edges: HashMap<String, Option<EdgeOverride>>,
}

#[derive(Debug, Deserialize)]
pub struct StyleUpdate {
    #[serde(default)]
    node_styles: HashMap<String, Option<NodeStylePatch>>,
    #[serde(default)]
    edge_styles: HashMap<String, Option<EdgeStyleOverride>>,
}

// Tauri commands will be added here

#[tauri::command]
pub fn load_diagram(
    path: String,
    state: tauri::State<'_, AppState>,
)
-> Result<DiagramPayload, String> {
    let path_buf = PathBuf::from(&path);

    // Read file contents
    let contents = std::fs::read_to_string(&path_buf)
        .map_err(|e| format!("failed to read '{}': {e}", path_buf.display()))?;

    // Split source and overrides, then parse diagram definition
    let (definition, parsed_overrides) =
        split_source_and_overrides(&contents).map_err(|e| e.to_string())?;
    let diagram = Diagram::parse(&definition).map_err(|e| e.to_string())?;

    // Update AppState with new path and overrides
    {
        if let Ok(mut guard) = state.source_path.lock() {
            *guard = Some(path_buf.clone());
        }
        if let Ok(mut guard) = state.overrides.lock() {
            *guard = parsed_overrides.clone();
        }
    }

    // Read current overrides/background from state
    let overrides_snapshot = {
        state
            .overrides
            .lock()
            .map(|g| g.clone())
            .map_err(|_| "failed to access overrides state".to_string())?
    };
    let background = state
        .background
        .lock()
        .map(|g| g.clone())
        .map_err(|_| "failed to access background state".to_string())?;

    // Compute layout and geometry
    let layout = diagram
        .layout(Some(&overrides_snapshot))
        .map_err(|e| e.to_string())?;
    let geometry = align_geometry(
        &layout.final_positions,
        &layout.final_routes,
        &diagram.edges,
        &diagram.subgraphs,
    )
    .map_err(|e| e.to_string())?;

    // Build node payloads
    let mut nodes = Vec::new();
    for id in &diagram.order {
        let node = diagram
            .nodes
            .get(id)
            .ok_or_else(|| format!("node '{}' missing from diagram", id))?;
        let auto_position = layout
            .auto_positions
            .get(id)
            .copied()
            .ok_or_else(|| format!("auto layout missing node '{}'", id))?;
        let final_position = layout
            .final_positions
            .get(id)
            .copied()
            .ok_or_else(|| format!("final layout missing node '{}'", id))?;
        let override_position = overrides_snapshot.nodes.get(id).copied();
        let style = overrides_snapshot.node_styles.get(id);
        let fill_color = style.and_then(|s| s.fill.clone());
        let stroke_color = style.and_then(|s| s.stroke.clone());
        let text_color = style.and_then(|s| s.text.clone());
        nodes.push(NodePayload {
            id: id.clone(),
            label: node.label.clone(),
            shape: node.shape.as_str().to_string(),
            auto_position,
            rendered_position: final_position,
            override_position,
            fill_color,
            stroke_color,
            text_color,
            membership: diagram
                .node_membership
                .get(id)
                .cloned()
                .unwrap_or_default(),
        });
    }

    // Build edge payloads
    let mut edges = Vec::new();
    for edge in &diagram.edges {
        let identifier = edge_identifier(edge);
        let auto_points = layout
            .auto_routes
            .get(&identifier)
            .cloned()
            .unwrap_or_default();
        let final_points = layout
            .final_routes
            .get(&identifier)
            .cloned()
            .unwrap_or_default();
        let manual_points = overrides_snapshot
            .edges
            .get(&identifier)
            .map(|edge_override| edge_override.points.clone());
        let style = overrides_snapshot.edge_styles.get(&identifier);
        let line_kind = style
            .and_then(|s| s.line)
            .unwrap_or(edge.kind)
            .as_str()
            .to_string();
        let color = style.and_then(|s| s.color.clone());
        let arrow_direction = style
            .and_then(|s| s.arrow)
            .map(|direction| direction.as_str().to_string());

        edges.push(EdgePayload {
            id: identifier,
            from: edge.from.clone(),
            to: edge.to.clone(),
            label: edge.label.clone(),
            kind: line_kind,
            auto_points,
            rendered_points: final_points,
            override_points: manual_points,
            color,
            arrow_direction,
            line_style: None,
        });
    }

    // Build subgraph payloads
    let mut subgraphs = Vec::new();
    for sg in &geometry.subgraphs {
        subgraphs.push(SubgraphPayload {
            id: sg.id.clone(),
            label: sg.label.clone(),
            x: sg.x,
            y: sg.y,
            width: sg.width,
            height: sg.height,
            label_x: sg.label_x,
            label_y: sg.label_y,
            depth: sg.depth,
            order: sg.order,
            parent_id: sg.parent_id.clone(),
        });
    }

    let payload = DiagramPayload {
        source_path: path_buf.display().to_string(),
        background,
        auto_size: layout.auto_size,
        render_size: CanvasSize {
            width: geometry.width,
            height: geometry.height,
        },
        nodes,
        edges,
        subgraphs,
        source: contents,
    };

    Ok(payload)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Commands will be registered here
            load_diagram,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
