//! The Document canvas: REDA's wgpu renderer (`reda_render::gpu`) inside an `egui_wgpu`
//! paint callback.
//!
//! - [`Camera`]: the view transform and the input that moves it (wheel zoom at the cursor,
//!   right/middle-drag pan).
//! - [`CanvasResources`]: the GPU side, kept in egui-wgpu's callback resources. Each canvas
//!   renders into its own 4× MSAA offscreen target, re-recorded only when the Scene,
//!   theme, view, size or grid changed, and copied into the window every frame.
//! - [`canvas`]: the widget that ties them together for a Document tab.

mod camera;
mod gpu;
pub mod interact;
pub mod markers;
mod widget;

pub use camera::{Camera, CanvasInput, grid_pitch};
pub use gpu::{
    CanvasFrame, CanvasGpu, CanvasResources, LiveHandle, PaletteHandle, SceneHandle,
    SelectionHandle, paint,
};
pub(crate) use markers::MARKER_RADIUS;
pub use widget::{
    CanvasInputs, Cutter, Overlay, RubberBand, SEGMENT_HANDLE, canvas, canvas_id, canvas_input,
    canvas_paint,
};
