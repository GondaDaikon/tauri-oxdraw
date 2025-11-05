# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Oxdraw is a declarative diagramming tool that combines Mermaid syntax with visual editing capabilities. The project consists of:

- **Rust CLI/library**: Parses `.mmd` files, computes layouts, and renders to SVG/PNG
- **Next.js frontend**: Interactive web editor for visual diagram manipulation

Key concept: Visual tweaks in the editor are persisted back to source files as declarative comments (`%% OXDRAW LAYOUT START/END` blocks), keeping diagrams deterministic and version-controlled.

## Build & Development Commands

### Frontend (Next.js)

```bash
cd frontend
npm install          # Install dependencies
npm run dev          # Start dev server with Turbopack
npm run build        # Production build to frontend/out/
```

### Rust CLI

```bash
# Build the CLI (requires frontend to be built first)
cargo build          # Debug build
cargo build --release  # Optimized build

# Build without server feature (lightweight, rendering-only)
cargo build --no-default-features

# Run tests
cargo test           # Run all tests
cargo test --test cli  # Run CLI integration tests
cargo test --test library  # Run library unit tests
cargo test --test wasm_compatibility  # Run WASM tests

# Run specific test
cargo test <test_name>  # Run a single test by name

# WASM support
rustup target add wasm32-unknown-unknown
wasm-pack build --target web --no-default-features --out-dir pkg
wasm-pack test --node --no-default-features
```

### Running Oxdraw

```bash
# Render diagram
oxdraw --input diagram.mmd         # Outputs diagram.mmd.svg
oxdraw -i diagram.mmd --png        # Outputs diagram.mmd.png
oxdraw -i diagram.mmd -o out.svg   # Custom output path

# Interactive editor
oxdraw --input diagram.mmd --edit  # Launch editor at http://127.0.0.1:5151
oxdraw --new                       # Create new diagram and open editor
oxdraw --new -i mydiagram.mmd      # Create with custom name
```

## Architecture

### Rust Core (src/)

- **lib.rs**: Core data structures and constants (node dimensions, spacing, layout margins)
- **diagram.rs**: Mermaid parser, layout algorithm (Sugiyama-style layered graph layout), and SVG/PNG rendering
- **cli.rs**: CLI argument parsing, file I/O, and editor launcher
- **serve.rs**: Axum HTTP server for editor mode (behind `server` feature flag)
- **utils.rs**: Layout override persistence (serialization/deserialization of position/style overrides)

### Frontend (frontend/)

- Next.js 15 with React 19 and Tailwind CSS v4
- **app/page.tsx**: Main editor component (canvas, node/edge manipulation)
- Built output goes to `frontend/out/` and is embedded in the Rust binary via `build.rs`

### Key Data Flow

1. Parse `.mmd` file → `Diagram` struct (nodes, edges, subgraphs)
2. Extract layout overrides from `%% OXDRAW LAYOUT` comment blocks → `LayoutOverrides`
3. Compute auto-layout positions → merge with manual overrides → `Geometry`
4. Render to SVG/PNG or serve via HTTP for editing
5. Editor changes auto-save back to source file with updated override comments

### Layout Algorithm

- Implements Sugiyama-style layered graph layout for flowcharts
- Nodes are arranged in ranks (layers) based on topological ordering
- Edge routing uses orthogonal paths with collision avoidance
- Manual adjustments (node positions, edge waypoints) are stored in `LayoutOverrides`
- Subgraphs have computed bounding boxes with padding

### Features System

- Default feature: `server` (enables Axum web server and `--edit` mode)
- Build without `server` for lightweight rendering-only binary
- WASM builds must use `--no-default-features` (no server, no tokio)

## Testing

- **tests/cli.rs**: CLI integration tests using `assert_cmd`
- **tests/library.rs**: Core library unit tests
- **tests/wasm_compatibility.rs**: WASM compatibility tests (use `wasm-pack test`)
- Test fixtures in `tests/input/` and `tests/output/`

## Environment Variables

- `OXDRAW_WEB_DIST`: Override path to frontend build directory (defaults to `frontend/out/`)
- Set this if frontend assets are in a custom location

## Important Constants (lib.rs)

Layout behavior is controlled by constants like:

- `NODE_WIDTH`, `NODE_HEIGHT`: Default node dimensions (140x60)
- `NODE_SPACING`: Horizontal spacing between nodes (160)
- `SUBGRAPH_PADDING`: Padding inside subgraph containers (48)
- `EDGE_COLLISION_MARGIN`: Minimum clearance for edge routing (6)

Adjust these to modify global layout characteristics.

## Common Patterns

### Adding New Node Shapes

1. Add variant to `NodeShape` enum in lib.rs
2. Implement shape rendering logic in `diagram.rs` (SVG path generation)
3. Update parser in `Diagram::parse()` to recognize new Mermaid syntax

### Modifying Layout Algorithm

- Core layout logic is in `diagram.rs`: `compute_auto_layout()`, `compute_geometry()`
- Edge routing: `route_edge()` function with orthogonal pathfinding
- Test changes against fixtures in `tests/input/` to avoid regressions

### Editor API Changes

- Server endpoints are in `serve.rs`: `/api/parse`, `/api/save`, `/api/render`
- Frontend calls these via fetch from `app/page.tsx`
- Changes require rebuilding both frontend and Rust components
