//! Orbit camera around a focal point on the ground, ported from tyke's MixedCamera.
//!
//! The camera blends smoothly between perspective and orthographic with a dolly
//! zoom. As the field of view shrinks, the camera backs away so that the visible
//! height at the focal point (`view_height`) stays the same. Once the FOV reaches
//! its minimum, the camera switches to a true orthographic projection.

use glam::camera::rh::{proj::directx as proj, view};
use glam::{Mat4, Vec3};
use std::f32::consts::PI;

pub const PERSPECTIVE_FOV: f32 = PI / 3.0;
const MIN_FOV: f32 = 0.02;
/// FOV change per second while transitioning.
const FOV_SPEED: f32 = 4.0;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Camera {
    /// The point on the ground the camera orbits and looks at.
    pub focus: Vec3,
    pub yaw: f32,
    /// Negative is looking down.
    pub pitch: f32,
    /// World-space height visible at the focal point; this is the zoom level.
    pub view_height: f32,
    pub fov: f32,
    pub want_ortho: bool,
}

impl Camera {
    pub fn new(focus: Vec3) -> Camera {
        Camera {
            focus,
            yaw: PI / 6.0,
            pitch: -PI / 5.0,
            view_height: 50.0,
            fov: PERSPECTIVE_FOV,
            want_ortho: false,
        }
    }

    pub fn is_ortho(&self) -> bool {
        self.fov <= MIN_FOV
    }

    /// Advances the FOV transition. Returns true while it is still animating.
    pub fn update(&mut self, dt: f32) -> bool {
        let target = if self.want_ortho { MIN_FOV } else { PERSPECTIVE_FOV };
        if self.fov == target {
            return false;
        }
        let step = FOV_SPEED * dt;
        self.fov = if self.fov < target {
            (self.fov + step).min(target)
        } else {
            (self.fov - step).max(target)
        };
        true
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            -self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    /// Distance from the focal point that gives `view_height` at the current FOV.
    pub fn distance(&self) -> f32 {
        self.view_height / (2.0 * (0.5 * self.fov).tan())
    }

    pub fn eye(&self) -> Vec3 {
        self.focus - self.forward() * self.distance()
    }

    /// Combined view-projection matrix. `scene_radius` bounds everything that
    /// should be visible and is used to choose tight near/far planes.
    pub fn view_proj(&self, aspect: f32, scene_radius: f32) -> Mat4 {
        let dist = self.distance();
        let view = view::look_at_mat4(self.eye(), self.focus, Vec3::Y);
        let near = (dist - scene_radius).max(dist * 0.002).max(0.05);
        let far = dist + scene_radius;
        let proj = if self.is_ortho() {
            // Same visible height as the perspective view at the focus (the FOV
            // is vertical), so switching to true ortho doesn't jump.
            let h = self.view_height * 0.5;
            let w = h * aspect;
            proj::orthographic(-w, w, -h, h, near, far)
        } else {
            proj::perspective(self.fov, aspect, near, far)
        };
        proj * view
    }

    /// Moves the focus in the camera's ground-plane frame. `right` and `back`
    /// are in world units.
    pub fn pan(&mut self, right: f32, back: f32) {
        let (s, c) = self.yaw.sin_cos();
        self.focus.x += c * right + s * back;
        self.focus.z += -s * right + c * back;
    }

    pub fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw = (self.yaw + dyaw).rem_euclid(2.0 * PI);
        self.pitch = (self.pitch + dpitch).clamp(-1.5, -0.12);
    }

    pub fn zoom(&mut self, factor: f32, max_height: f32) {
        self.view_height = (self.view_height * factor).clamp(4.0, max_height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_to_ortho_does_not_jump() {
        let mut cam = Camera::new(Vec3::new(50.0, 5.0, 50.0));
        let aspect = 16.0 / 9.0;
        let point = cam.focus + Vec3::new(7.0, 2.0, -4.0);
        let project = |cam: &Camera| cam.view_proj(aspect, 400.0).project_point3(point).truncate();
        cam.fov = MIN_FOV * 1.001;
        assert!(!cam.is_ortho());
        let before = project(&cam);
        cam.fov = MIN_FOV;
        assert!(cam.is_ortho());
        let after = project(&cam);
        assert!((before - after).length() < 0.01, "{before} vs {after}");
    }
}
