//! Physical key tracking, so positional controls (WASD and friends) stay in
//! the same place on any keyboard layout (Dvorak, AZERTY, ...).

use eframe::egui::{self, Key};
use std::collections::HashMap;

/// Keys currently held, by physical position (named after their QWERTY key).
#[derive(Default)]
pub struct PhysicalKeys {
    /// Physical key -> the logical key it produced, used to match releases
    /// that only report a logical key.
    down: HashMap<Key, Key>,
}

impl PhysicalKeys {
    /// Updates from this frame's input events.
    pub fn update(&mut self, input: &egui::InputState) {
        for event in &input.events {
            match *event {
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    ..
                } => {
                    let physical = physical_key.unwrap_or(key);
                    if pressed {
                        self.down.insert(physical, key);
                    } else if physical_key.is_some() {
                        self.down.remove(&physical);
                    } else {
                        self.down.retain(|_, logical| *logical != key);
                    }
                }
                // Releases are lost while unfocused, so forget everything.
                egui::Event::WindowFocused(false) => self.down.clear(),
                _ => {}
            }
        }
    }

    /// Whether the key in this physical position (on a QWERTY layout) is held.
    pub fn down(&self, key: Key) -> bool {
        self.down.contains_key(&key)
    }

    pub fn any_down(&self) -> bool {
        !self.down.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(key: Key, physical_key: Option<Key>, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn dvorak_comma_is_qwerty_w() {
        let mut keys = PhysicalKeys::default();
        let mut input = egui::InputState::default();
        // On Dvorak, the key in QWERTY's W position types a comma.
        input.events = vec![key_event(Key::Comma, Some(Key::W), true)];
        keys.update(&input);
        assert!(keys.down(Key::W));
        assert!(!keys.down(Key::Comma));
        // A release that only reports the logical key still clears it.
        input.events = vec![key_event(Key::Comma, None, false)];
        keys.update(&input);
        assert!(!keys.any_down());
    }
}
