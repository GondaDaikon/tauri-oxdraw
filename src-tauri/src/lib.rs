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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Commands will be registered here
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
