use std::collections::HashSet;

use glam::Vec2;

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard/mouse input state, updated each frame from web events.
pub struct InputState {
    /// Keys currently held down.
    pub keys_down: HashSet<String>,
    /// Keys pressed this frame (cleared each frame).
    pub keys_just_pressed: HashSet<String>,
    /// Keys released this frame (cleared each frame).
    pub keys_just_released: HashSet<String>,
    /// Mouse buttons currently held.
    pub mouse_buttons: HashSet<MouseButton>,
    /// Mouse buttons pressed this frame.
    pub mouse_just_pressed: HashSet<MouseButton>,
    /// Mouse buttons released this frame.
    pub mouse_just_released: HashSet<MouseButton>,
    /// Cursor position in CSS pixels relative to canvas.
    pub cursor_position: Vec2,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_down: HashSet::new(),
            keys_just_pressed: HashSet::new(),
            keys_just_released: HashSet::new(),
            mouse_buttons: HashSet::new(),
            mouse_just_pressed: HashSet::new(),
            mouse_just_released: HashSet::new(),
            cursor_position: Vec2::ZERO,
        }
    }

    /// Called at the start of each frame to register a key down event.
    pub fn on_key_down(&mut self, code: String) {
        if self.keys_down.insert(code.clone()) {
            self.keys_just_pressed.insert(code);
        }
    }

    /// Called when a key is released.
    pub fn on_key_up(&mut self, code: String) {
        self.keys_down.remove(&code);
        self.keys_just_released.insert(code);
    }

    /// Called on mouse button press.
    pub fn on_mouse_down(&mut self, button: MouseButton) {
        self.mouse_buttons.insert(button);
        self.mouse_just_pressed.insert(button);
    }

    /// Called on mouse button release.
    pub fn on_mouse_up(&mut self, button: MouseButton) {
        self.mouse_buttons.remove(&button);
        self.mouse_just_released.insert(button);
    }

    /// Called on mouse move.
    pub fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.cursor_position = Vec2::new(x, y);
    }

    /// Check if a key is currently held.
    pub fn is_key_down(&self, code: &str) -> bool {
        self.keys_down.contains(code)
    }

    /// Check if a key was pressed this frame.
    pub fn is_key_just_pressed(&self, code: &str) -> bool {
        self.keys_just_pressed.contains(code)
    }

    /// Check if a mouse button is held.
    pub fn is_mouse_down(&self, button: MouseButton) -> bool {
        self.mouse_buttons.contains(&button)
    }

    /// Check if a mouse button was pressed this frame.
    pub fn is_mouse_just_pressed(&self, button: MouseButton) -> bool {
        self.mouse_just_pressed.contains(&button)
    }

    /// Check if a mouse button was released this frame.
    pub fn is_mouse_just_released(&self, button: MouseButton) -> bool {
        self.mouse_just_released.contains(&button)
    }

    /// Clear per-frame state. Call at the end of each frame.
    pub fn end_frame(&mut self) {
        self.keys_just_pressed.clear();
        self.keys_just_released.clear();
        self.mouse_just_pressed.clear();
        self.mouse_just_released.clear();
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_down_and_up() {
        let mut input = InputState::new();
        input.on_key_down("KeyA".into());
        assert!(input.is_key_down("KeyA"));
        assert!(input.is_key_just_pressed("KeyA"));

        input.end_frame();
        assert!(input.is_key_down("KeyA"));
        assert!(!input.is_key_just_pressed("KeyA"));

        input.on_key_up("KeyA".into());
        assert!(!input.is_key_down("KeyA"));
    }

    #[test]
    fn mouse_press_and_release() {
        let mut input = InputState::new();
        input.on_mouse_down(MouseButton::Left);
        assert!(input.is_mouse_down(MouseButton::Left));
        assert!(input.is_mouse_just_pressed(MouseButton::Left));

        input.end_frame();
        assert!(!input.is_mouse_just_pressed(MouseButton::Left));

        input.on_mouse_up(MouseButton::Left);
        assert!(!input.is_mouse_down(MouseButton::Left));
        assert!(input.is_mouse_just_released(MouseButton::Left));
    }

    #[test]
    fn cursor_position_tracks() {
        let mut input = InputState::new();
        input.on_mouse_move(100.0, 200.0);
        assert_eq!(input.cursor_position, Vec2::new(100.0, 200.0));
    }

    #[test]
    fn duplicate_key_down_not_just_pressed_twice() {
        let mut input = InputState::new();
        input.on_key_down("KeyA".into());
        input.on_key_down("KeyA".into()); // duplicate
        assert!(input.is_key_just_pressed("KeyA"));
        // Should still only be counted once
        assert_eq!(input.keys_just_pressed.len(), 1);
    }
}
