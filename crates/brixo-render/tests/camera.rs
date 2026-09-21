// The new glam functions must produce exactly the old matrices.
#[allow(deprecated)]
#[test]
fn camera_matrices_unchanged() {
    use glam::{Mat4, Vec3};
    let cam = brixo_render::Camera::new();
    let old = Mat4::perspective_rh(70f32.to_radians(), 1.6, 0.1, 2000.0)
        * Mat4::look_to_rh(cam.position, cam.forward(), Vec3::Y);
    let new = cam.view_proj(1.6);
    assert!(old.abs_diff_eq(new, 1e-6), "{old:?}\n{new:?}");
}
