# Document canvases are drawn by REDA's own wgpu renderer inside egui

The PCB and schematic canvases are drawn by a REDA renderer on wgpu, embedded in the egui window through an `egui_wgpu` paint callback. Document geometry is uploaded to the GPU once, as instanced primitives (capsule, rect, ring) with analytic anti-aliasing plus triangle lists for Zone fills, and is re-uploaded only when the Document changes. Each frame writes only the view transform. egui still draws the UI around the canvas and may draw light overlays on top of it.

A prototype ([`prototype/board-renderer`](https://github.com/openreda/reda/tree/prototype/board-renderer)) settled this. On a synthetic 2-layer board with 12k Tracks, 12k Pads and 4k Vias, rebuilding egui shapes every frame cost up to about 14 ms of CPU time per frame when zoomed out to the whole board, and about 65 ms at 8× that density. The wgpu path held about 0.3 ms at every density and zoom.

## Considered Options

- **egui `Painter` shapes rebuilt every frame**: rejected. CPU tessellation grows with what is on screen, so the whole-board view, the most common one, is the slowest.
- **A world-space egui `Mesh` built once and moved to screen positions every frame**: rejected. It still moves every vertex every frame (slower than shapes in the prototype) and loses egui's anti-aliasing.
- **The painter for schematics, wgpu only for PCBs**: rejected so there is one canvas renderer to maintain.

## Consequences

- `reda-render` builds the GPU-ready buffers and owns the shaders and wgpu pipelines. It depends on wgpu, not on egui, so it still builds for wasm32 (ADR-0009). `reda-ui` wraps it in the `egui_wgpu` callback.
- Text on the canvas (Designators, silkscreen, fonts), Zone fill triangulation, and selection and hover highlighting must be designed for this renderer rather than borrowed from egui.
- eframe must use its wgpu backend, not glow.
