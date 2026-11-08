//! Vengeance infrastructure blueprint: hosts as rack cabinets, namespaces as rack units,
//! workloads as modules, images as chips, drawn as a blueprint with raw WebGL2.
//!
//! The contract with the page lives in `docs/topology.md` (sections 2 and 3).

pub mod camera;
pub mod engine;
mod engine_walk;
pub mod geo;
pub mod layout;
pub mod room;
pub mod walk;
pub mod scene;

#[cfg(target_arch = "wasm32")]
mod gl;

#[cfg(target_arch = "wasm32")]
pub use wasm::Blueprint;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::Cell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{HtmlCanvasElement, KeyboardEvent};

    use crate::engine::Engine;
    use crate::gl::Renderer;

    type KeyClosure = Closure<dyn FnMut(KeyboardEvent)>;

    #[wasm_bindgen]
    pub struct Blueprint {
        engine: Engine,
        renderer: Renderer,
        shift: Rc<Cell<bool>>,
        listeners: Vec<(&'static str, KeyClosure)>,
    }

    #[wasm_bindgen]
    impl Blueprint {
        /// Creates a WebGL2 context on the canvas. Throws when WebGL2 is unavailable.
        #[wasm_bindgen(constructor)]
        pub fn new(canvas: HtmlCanvasElement) -> Result<Blueprint, JsValue> {
            let renderer = Renderer::new(&canvas).map_err(|e| JsValue::from(js_sys::Error::new(&e)))?;
            let mut engine = Engine::new();
            let (w, h) = (canvas.client_width().max(1) as f32, canvas.client_height().max(1) as f32);
            engine.resize(w, h, 1.0);
            // Shift+drag pans; pointer events from the page do not carry modifiers, so track them here.
            let shift = Rc::new(Cell::new(false));
            let mut listeners: Vec<(&'static str, KeyClosure)> = Vec::new();
            if let Some(win) = web_sys::window() {
                for ev in ["keydown", "keyup"] {
                    let s = shift.clone();
                    let cb: KeyClosure = Closure::new(move |e: KeyboardEvent| s.set(e.shift_key()));
                    let _ = win.add_event_listener_with_callback(ev, cb.as_ref().unchecked_ref());
                    listeners.push((ev, cb));
                }
            }
            Ok(Blueprint { engine, renderer, shift, listeners })
        }

        /// (Re)builds the geometry. Keeps the camera when the focused/visible ids persist.
        /// Invalid JSON keeps the previous scene (and logs a warning).
        pub fn set_scene(&mut self, scene_json: &str) {
            if let Err(e) = self.engine.set_scene(scene_json) {
                web_sys::console::warn_1(&format!("blueprint: ignoring scene: {e}").into());
            }
        }

        pub fn resize(&mut self, css_width: f32, css_height: f32, device_pixel_ratio: f32) {
            self.engine.resize(css_width, css_height, device_pixel_ratio);
            self.renderer.resize(self.engine.css_w, self.engine.css_h, self.engine.dpr);
        }

        pub fn set_reduced_motion(&mut self, reduced: bool) {
            self.engine.reduced_motion = reduced;
            self.engine.mark_dirty();
        }

        pub fn frame(&mut self, time_ms: f64) {
            if self.renderer.lost() {
                return;
            }
            if self.engine.update(time_ms) {
                self.renderer.draw(&self.engine);
            }
        }

        pub fn pointer_down(&mut self, x: f32, y: f32, button: i32) {
            self.engine.shift = self.shift.get();
            self.engine.pointer_down(x, y, button);
        }

        pub fn pointer_move(&mut self, x: f32, y: f32) {
            self.engine.pointer_move(x, y);
        }

        pub fn pointer_up(&mut self, x: f32, y: f32, button: i32) {
            self.engine.pointer_up(x, y, button);
        }

        pub fn wheel(&mut self, delta_y: f32, x: f32, y: f32) {
            self.engine.wheel(delta_y, x, y);
        }

        pub fn key(&mut self, code: &str) {
            self.engine.key(code);
        }

        pub fn focus(&mut self, id: &str) {
            self.engine.focus_id(id);
        }

        pub fn reset_view(&mut self) {
            self.engine.reset_view();
        }

        pub fn hovered(&self) -> Option<String> {
            self.engine.hovered_id()
        }

        pub fn selected(&self) -> Option<String> {
            self.engine.selected_id()
        }

        pub fn labels(&self) -> String {
            self.engine.labels_json()
        }

        /// 'orbit' | 'walk'. Walk spawns at the entrance facing the racks; orbit restores the
        /// orbit camera as it was.
        pub fn set_mode(&mut self, mode: &str) {
            self.engine.set_mode(mode);
        }

        pub fn mode(&self) -> String {
            self.engine.mode_name().to_string()
        }

        /// Walk mode: KeyboardEvent.code pressed (WASD / arrows move and turn, Shift runs, E interacts).
        pub fn key_down(&mut self, code: &str) {
            self.engine.key_down(code);
        }

        pub fn key_up(&mut self, code: &str) {
            self.engine.key_up(code);
        }

        /// Walk mode: pointer-lock movementX / movementY in CSS pixels.
        pub fn look(&mut self, dx: f32, dy: f32) {
            self.engine.look(dx, dy);
        }

        /// Walk mode: select what the crosshair aims at.
        pub fn interact(&mut self) {
            self.engine.interact();
        }

        /// JSON floor plan with the player position for a 2D minimap.
        pub fn minimap(&self) -> String {
            self.engine.minimap_json()
        }
    }

    impl Drop for Blueprint {
        fn drop(&mut self) {
            if let Some(win) = web_sys::window() {
                for (ev, cb) in self.listeners.drain(..) {
                    let _ = win.remove_event_listener_with_callback(ev, cb.as_ref().unchecked_ref());
                }
            }
        }
    }
}
