//! Shared filled-area rendering for curves and waveforms.

use egui::epaint::{Vertex, WHITE_UV};
use egui::{Color32, Mesh, Painter, Pos2, Shape, pos2};

/// Fills the area between a polyline and a horizontal baseline.
///
/// The whole area is emitted as one mesh, so adjacent segments share edges
/// instead of compositing separate translucent shapes. Close or crossing
/// segments therefore keep a uniform opacity.
pub(crate) fn fill_polyline_area(
    painter: &Painter,
    points: &[Pos2],
    baseline: f32,
    color: Color32,
) {
    if points.len() < 2 {
        return;
    }
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(points.len().saturating_sub(1) * 4);
    mesh.reserve_triangles(points.len().saturating_sub(1) * 2);
    for pair in points.windows(2) {
        add_area_segment(&mut mesh, pair[0], pair[1], baseline, color);
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}

fn add_area_segment(mesh: &mut Mesh, from: Pos2, to: Pos2, zero: f32, color: Color32) {
    let from_side = from.y - zero;
    let to_side = to.y - zero;
    if from_side == 0.0 && to_side == 0.0 {
        return;
    }
    if from_side.signum() == to_side.signum() || to_side == 0.0 {
        push_quad(mesh, pos2(from.x, zero), pos2(to.x, zero), to, from, color);
        return;
    }
    if from_side == 0.0 {
        push_triangle(mesh, pos2(from.x, zero), pos2(to.x, zero), to, color);
        return;
    }
    let crossing = (zero - from.y) / (to.y - from.y);
    let x = from.x + (to.x - from.x) * crossing.clamp(0.0, 1.0);
    push_triangle(mesh, pos2(from.x, zero), pos2(x, zero), from, color);
    push_triangle(mesh, pos2(x, zero), pos2(to.x, zero), to, color);
}

fn push_triangle(mesh: &mut Mesh, a: Pos2, b: Pos2, c: Pos2, color: Color32) {
    let base = mesh.vertices.len() as u32;
    for pos in [a, b, c] {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color,
        });
    }
    mesh.add_triangle(base, base + 1, base + 2);
}

fn push_quad(mesh: &mut Mesh, a: Pos2, b: Pos2, c: Pos2, d: Pos2, color: Color32) {
    let base = mesh.vertices.len() as u32;
    for pos in [a, b, c, d] {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color,
        });
    }
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}
