use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use oxdraw::*;
use anyhow::{anyhow, Context};
use tauri_plugin_dialog::DialogExt;

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

#[derive(Debug, Deserialize, Default)]
pub struct EdgeStylePatch {
    #[serde(default)]
    line: Option<Option<EdgeKind>>,
    #[serde(default)]
    color: Option<Option<String>>,
    #[serde(default)]
    arrow: Option<Option<EdgeArrowDirection>>,
}

#[derive(Debug, Deserialize)]
pub struct StyleUpdate {
    #[serde(default)]
    node_styles: HashMap<String, Option<NodeStylePatch>>,
    #[serde(default)]
    edge_styles: HashMap<String, Option<EdgeStylePatch>>,
}

#[derive(Debug, Serialize)]
pub struct SourcePayload {
    source: String,
}

// Helper functions

#[tauri::command]
fn load_diagram(
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

// Helper function to merge source with overrides
fn merge_source_and_overrides(definition: &str, overrides: &LayoutOverrides) -> anyhow::Result<String> {
    let trimmed = definition.trim_end_matches('\n');
    let mut output = trimmed.to_string();
    output.push('\n');

    if overrides.is_empty() {
        return Ok(output);
    }

    output.push('\n');
    output.push_str(LAYOUT_BLOCK_START);
    output.push('\n');

    let json = serde_json::to_string_pretty(overrides)?;
    for line in json.lines() {
        output.push_str("%% ");
        output.push_str(line);
        output.push('\n');
    }

    output.push_str(LAYOUT_BLOCK_END);
    output.push('\n');

    Ok(output)
}

// Helper function to build diagram payload
fn build_diagram_payload(
    source_path: &PathBuf,
    background: &str,
    diagram: &Diagram,
    overrides: &LayoutOverrides,
    source: String,
) -> anyhow::Result<DiagramPayload> {
    let layout = diagram.layout(Some(overrides))?;
    let geometry = align_geometry(
        &layout.final_positions,
        &layout.final_routes,
        &diagram.edges,
        &diagram.subgraphs,
    )?;

    let mut nodes = Vec::new();
    for id in &diagram.order {
        let node = diagram
            .nodes
            .get(id)
            .ok_or_else(|| anyhow!("node '{}' missing from diagram", id))?;
        let auto_position = layout
            .auto_positions
            .get(id)
            .copied()
            .ok_or_else(|| anyhow!("auto layout missing node '{}'", id))?;
        let final_position = layout
            .final_positions
            .get(id)
            .copied()
            .ok_or_else(|| anyhow!("final layout missing node '{}'", id))?;
        let override_position = overrides.nodes.get(id).copied();
        let style = overrides.node_styles.get(id);
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
            membership: diagram.node_membership.get(id).cloned().unwrap_or_default(),
        });
    }

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
        let manual_points = overrides
            .edges
            .get(&identifier)
            .map(|edge_override| edge_override.points.clone());
        let style = overrides.edge_styles.get(&identifier);
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
        source_path: source_path.display().to_string(),
        background: background.to_string(),
        auto_size: layout.auto_size,
        render_size: CanvasSize {
            width: geometry.width,
            height: geometry.height,
        },
        nodes,
        edges,
        subgraphs,
        source,
    };

    Ok(payload)
}

#[tauri::command]
fn update_style(state: tauri::State<AppState>, payload: StyleUpdate) -> Result<DiagramPayload, String> {
    // Get source path
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    // Apply style patches to overrides in memory
    let snapshot = {
        let mut overrides = state
            .overrides
            .lock()
            .map_err(|_| "failed to lock overrides".to_string())?;

        for (id, value) in payload.node_styles.into_iter() {
            match value {
                Some(patch) => {
                    let mut current = overrides.node_styles.remove(&id).unwrap_or_default();

                    if let Some(fill) = patch.fill {
                        current.fill = fill;
                    }
                    if let Some(stroke) = patch.stroke {
                        current.stroke = stroke;
                    }
                    if let Some(text) = patch.text {
                        current.text = text;
                    }

                    if current.is_empty() {
                        overrides.node_styles.remove(&id);
                    } else {
                        overrides.node_styles.insert(id, current);
                    }
                }
                None => {
                    overrides.node_styles.remove(&id);
                }
            }
        }

        for (id, value) in payload.edge_styles.into_iter() {
            match value {
                Some(patch) => {
                    let mut current = overrides.edge_styles.remove(&id).unwrap_or_default();

                    if let Some(line) = patch.line {
                        current.line = line;
                    }
                    if let Some(color) = patch.color {
                        current.color = color;
                    }
                    if let Some(arrow) = patch.arrow {
                        current.arrow = arrow;
                    }

                    if current.is_empty() {
                        overrides.edge_styles.remove(&id);
                    } else {
                        overrides.edge_styles.insert(id, current);
                    }
                }
                None => {
                    overrides.edge_styles.remove(&id);
                }
            }
        }

        overrides.clone()
    };

    // Persist to source file by merging definition with overrides
    let payload = (|| -> anyhow::Result<DiagramPayload> {
        let contents = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read '{}'", source_path.display()))?;
        let (definition, _) = split_source_and_overrides(&contents)?;
        let merged = merge_source_and_overrides(&definition, &snapshot)?;
        std::fs::write(&source_path, merged.as_bytes())
            .with_context(|| format!("failed to write '{}'", source_path.display()))?;

        // Recompute payload from definition + current overrides
        let diagram = Diagram::parse(&definition)?;
        let background = state
            .background
            .lock()
            .map_err(|_| anyhow!("failed to lock background"))?
            .clone();
        build_diagram_payload(&source_path, &background, &diagram, &snapshot, merged)
    })()
    .map_err(|e| e.to_string())?;

    Ok(payload)
}

#[tauri::command]
fn update_layout(state: tauri::State<AppState>, payload: LayoutUpdate) -> Result<DiagramPayload, String> {
    // Get source path
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    // Apply layout updates to overrides in memory
    let snapshot = {
        let mut overrides = state
            .overrides
            .lock()
            .map_err(|_| "failed to lock overrides".to_string())?;

        for (id, value) in payload.nodes.into_iter() {
            match value {
                Some(point) => {
                    overrides.nodes.insert(id, point);
                }
                None => {
                    overrides.nodes.remove(&id);
                }
            }
        }

        for (id, value) in payload.edges.into_iter() {
            match value {
                Some(edge_override) if !edge_override.points.is_empty() => {
                    overrides.edges.insert(id, edge_override);
                }
                _ => {
                    overrides.edges.remove(&id);
                }
            }
        }

        overrides.clone()
    };

    // Persist to source file by merging definition with overrides
    let payload = (|| -> anyhow::Result<DiagramPayload> {
        let contents = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read '{}'", source_path.display()))?;
        let (definition, _) = split_source_and_overrides(&contents)?;
        let merged = merge_source_and_overrides(&definition, &snapshot)?;
        std::fs::write(&source_path, merged.as_bytes())
            .with_context(|| format!("failed to write '{}'", source_path.display()))?;

        // Recompute payload from definition + current overrides
        let diagram = Diagram::parse(&definition)?;
        let background = state
            .background
            .lock()
            .map_err(|_| anyhow!("failed to lock background"))?
            .clone();
        build_diagram_payload(&source_path, &background, &diagram, &snapshot, merged)
    })()
    .map_err(|e| e.to_string())?;

    Ok(payload)
}

<<<<<<< HEAD
// Helper function to prune overrides for a diagram
fn prune_overrides_for_diagram(
    diagram: &Diagram,
    overrides: &mut LayoutOverrides,
) {
    use std::collections::HashSet;

    let node_ids: HashSet<String> = diagram.nodes.keys().cloned().collect();
    let edge_ids: HashSet<String> = diagram
        .edges
        .iter()
        .map(|edge| edge_identifier(edge))
        .collect();

    overrides.prune(&node_ids, &edge_ids);
}

#[tauri::command]
fn get_source(state: tauri::State<AppState>) -> Result<SourcePayload, String> {
    // Get source path
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    // Read current file contents
    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("failed to read '{}': {}", source_path.display(), e))?;

    Ok(SourcePayload { source })
}

#[tauri::command]
fn update_source(state: tauri::State<AppState>, source: String) -> Result<(), String> {
    // Get source path
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    // Parse and validate the new source
    let (definition, parsed_overrides) = split_source_and_overrides(&source)
        .map_err(|e| format!("failed to parse source: {}", e))?;

    let diagram = Diagram::parse(&definition)
        .map_err(|e| format!("failed to parse diagram: {}", e))?;

    // Check if source contains layout block
    let has_block = source
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case(LAYOUT_BLOCK_START));

    // Update overrides in state
    {
        let mut overrides = state
            .overrides
            .lock()
            .map_err(|_| "failed to lock overrides".to_string())?;

        if has_block {
            *overrides = parsed_overrides;
        }

        // Prune overrides to only include valid node/edge IDs
        let node_ids: std::collections::HashSet<String> = diagram.nodes.keys().cloned().collect();
        let edge_ids: std::collections::HashSet<String> = diagram
            .edges
            .iter()
            .map(|edge| edge_identifier(edge))
            .collect();

        overrides.prune(&node_ids, &edge_ids);
    }

    // Get updated overrides after pruning
    let snapshot = {
        state
            .overrides
            .lock()
            .map_err(|_| "failed to lock overrides".to_string())?
            .clone()
    };

    // Write definition with merged overrides to file
    let merged = merge_source_and_overrides(&definition, &snapshot)
        .map_err(|e| format!("failed to merge source and overrides: {}", e))?;

    std::fs::write(&source_path, merged.as_bytes())
        .map_err(|e| format!("failed to write '{}': {}", source_path.display(), e))?;

    Ok(())
}

#[tauri::command]
fn render_svg(state: tauri::State<AppState>) -> Result<String, String> {
    // Get source path and background
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    let background = state
        .background
        .lock()
        .map_err(|_| "failed to lock background".to_string())?
        .clone();

    // Get current overrides
    let overrides = state
        .overrides
        .lock()
        .map_err(|_| "failed to lock overrides".to_string())?
        .clone();

    // Read and parse diagram
    let contents = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("failed to read '{}': {}", source_path.display(), e))?;
    let (definition, _) = split_source_and_overrides(&contents).map_err(|e| e.to_string())?;
    let diagram = Diagram::parse(&definition).map_err(|e| e.to_string())?;

    // Render SVG with current overrides
    let override_ref = if overrides.is_empty() {
        None
    } else {
        Some(&overrides)
    };

    let svg = diagram
        .render_svg(&background, override_ref)
        .map_err(|e| e.to_string())?;

    Ok(svg)
}

#[tauri::command]
fn delete_node(state: tauri::State<AppState>, node_id: String) -> Result<DiagramPayload, String> {
    // Get source path
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    let payload = (|| -> anyhow::Result<DiagramPayload> {
        // Read and parse the current diagram
        let source = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read '{}'", source_path.display()))?;
        let mut diagram = Diagram::parse(&source)?;

        // Check if we can delete this node (must have at least one node)
        if diagram.nodes.len() == 1 && diagram.nodes.contains_key(&node_id) {
            anyhow::bail!("diagram must contain at least one node");
        }

        // Attempt to remove the node
        if !diagram.remove_node(&node_id) {
            anyhow::bail!("node '{}' not found", node_id);
        }

        // Write the modified diagram back to file
        let rewritten = diagram.to_definition();
        std::fs::write(&source_path, rewritten.as_bytes())
            .with_context(|| format!("failed to write '{}'", source_path.display()))?;

        // Prune overrides to remove references to deleted node and orphaned edges
        let mut overrides = state
            .overrides
            .lock()
            .map_err(|_| anyhow!("failed to lock overrides"))?;
        prune_overrides_for_diagram(&diagram, &mut overrides);
        let snapshot = overrides.clone();
        drop(overrides);

        // Merge the definition with cleaned overrides
        let merged = merge_source_and_overrides(&rewritten, &snapshot)?;
        std::fs::write(&source_path, merged.as_bytes())
            .with_context(|| format!("failed to write '{}'", source_path.display()))?;

        // Build and return the updated payload
        let background = state
            .background
            .lock()
            .map_err(|_| anyhow!("failed to lock background"))?
            .clone();
        build_diagram_payload(&source_path, &background, &diagram, &snapshot, merged)
    })()
    .map_err(|e| e.to_string())?;

    Ok(payload)
}

#[tauri::command]
fn delete_edge(state: tauri::State<AppState>, edge_id: String) -> Result<DiagramPayload, String> {
    // Get source path
    let source_path = {
        let guard = state
            .source_path
            .lock()
            .map_err(|_| "failed to lock source path".to_string())?;
        guard.clone().ok_or_else(|| "source path not set".to_string())?
    };

    let payload = (|| -> anyhow::Result<DiagramPayload> {
        // Read and parse the current diagram
        let source = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read '{}'", source_path.display()))?;
        let mut diagram = Diagram::parse(&source)?;

        // Attempt to remove the edge
        if !diagram.remove_edge_by_identifier(&edge_id) {
            anyhow::bail!("edge '{}' not found", edge_id);
        }

        // Write the modified diagram back to file
        let rewritten = diagram.to_definition();
        std::fs::write(&source_path, rewritten.as_bytes())
            .with_context(|| format!("failed to write '{}'", source_path.display()))?;

        // Prune overrides to remove references to deleted edge
        let mut overrides = state
            .overrides
            .lock()
            .map_err(|_| anyhow!("failed to lock overrides"))?;
        prune_overrides_for_diagram(&diagram, &mut overrides);
        let snapshot = overrides.clone();
        drop(overrides);

        // Merge the definition with cleaned overrides
        let merged = merge_source_and_overrides(&rewritten, &snapshot)?;
        std::fs::write(&source_path, merged.as_bytes())
            .with_context(|| format!("failed to write '{}'", source_path.display()))?;

        // Build and return the updated payload
        let background = state
            .background
            .lock()
            .map_err(|_| anyhow!("failed to lock background"))?
            .clone();
        build_diagram_payload(&source_path, &background, &diagram, &snapshot, merged)
    })()
    .map_err(|e| e.to_string())?;

    Ok(payload)
}

#[tauri::command]
async fn open_file_dialog(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Mermaid Diagrams", &["mmd"])
        .blocking_pick_file();

    Ok(file_path.and_then(|path| path.as_path().map(|p| p.display().to_string())))
}

#[tauri::command]
async fn save_file_dialog(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file_path = app
        .dialog()
        .file()
        .add_filter("Mermaid Diagrams", &["mmd"])
        .set_file_name("diagram.mmd")
        .blocking_save_file();

    Ok(file_path.and_then(|path| path.as_path().map(|p| p.display().to_string())))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_diagram,
            update_layout,
            update_style,
            get_source,
            update_source,
            render_svg,
            delete_node,
            delete_edge,
            open_file_dialog,
            save_file_dialog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
