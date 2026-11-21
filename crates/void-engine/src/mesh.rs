use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn append(&mut self, other: &Self, transform: Mat4) {
        let base = self.vertices.len() as u32;
        let normals = Mat3::from_mat4(transform).inverse().transpose();
        self.vertices.extend(other.vertices.iter().map(|v| {
            Vertex {
                position: transform
                    .transform_point3(Vec3::from(v.position))
                    .to_array(),
                normal: (normals * Vec3::from(v.normal)).normalize().to_array(),
                uv: v.uv,
            }
        }));
        self.indices.extend(other.indices.iter().map(|i| base + i));
    }
}

/// Rounded edges are geometry, so highlights and silhouettes survive close-ups.
pub fn bevel_box(half: Vec3, radius: f32) -> MeshData {
    bevel_box_with_uv(half, radius, None)
}

/// Beveled box with world-scale tiled UVs (`tile_meters` per 0..1 texture repeat) so faces never stretch.
pub fn bevel_box_tiled(half: Vec3, radius: f32, tile_meters: f32) -> MeshData {
    bevel_box_with_uv(half, radius, Some(tile_meters.max(0.001)))
}

fn bevel_box_with_uv(half: Vec3, radius: f32, tile_meters: Option<f32>) -> MeshData {
    let r = radius.max(0.00001).min(half.min_element() * 0.8);
    let core = half - Vec3::splat(r);
    let mut mesh = MeshData::default();
    for (n, u, v) in [
        (Vec3::X, -Vec3::Z, Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, -Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, -Vec3::X, Vec3::Y),
    ] {
        let hu = half.dot(u.abs());
        let hv = half.dot(v.abs());
        let hn = half.dot(n.abs());
        let xs = [-hu, -hu + r * 0.3, -hu + r, hu - r, hu - r * 0.3, hu];
        let ys = [-hv, -hv + r * 0.3, -hv + r, hv - r, hv - r * 0.3, hv];
        let base = mesh.vertices.len() as u32;
        for y in ys {
            for x in xs {
                let p = n * hn + u * x + v * y;
                let q = p.clamp(-core, core);
                let normal = (p - q).normalize();
                let uv = match tile_meters {
                    Some(tile) => [(x + hu) / tile, (hv - y) / tile],
                    None => [x / hu * 0.5 + 0.5, 0.5 - y / hv * 0.5],
                };
                mesh.vertices.push(Vertex {
                    position: (q + normal * r).to_array(),
                    normal: normal.to_array(),
                    uv,
                });
            }
        }
        for y in 0..5 {
            for x in 0..5 {
                let a = base + y * 6 + x;
                mesh.indices.extend([a, a + 1, a + 7, a, a + 7, a + 6]);
            }
        }
    }
    mesh
}

pub fn sphere(radius: f32, slices: u32, rings: u32) -> MeshData {
    let mut mesh = MeshData::default();
    for y in 0..=rings {
        let phi = y as f32 / rings as f32 * PI;
        for x in 0..=slices {
            let theta = x as f32 / slices as f32 * TAU;
            let n = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            mesh.vertices.push(Vertex {
                position: (n * radius).to_array(),
                normal: n.to_array(),
                uv: [x as f32 / slices as f32, y as f32 / rings as f32],
            });
        }
    }
    for y in 0..rings {
        for x in 0..slices {
            let a = y * (slices + 1) + x;
            let b = a + slices + 1;
            mesh.indices.extend([a, a + 1, b + 1, a, b + 1, b]);
        }
    }
    mesh
}

/// Revolves (height, radius) pairs around +Y; the profile is bottom to top.
pub fn lathe(profile: &[(f32, f32)], slices: u32, caps: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let height = (profile.last().unwrap().0 - profile[0].0).max(0.001);
    for (i, &(y, radius)) in profile.iter().enumerate() {
        let prev = profile[i.saturating_sub(1)];
        let next = profile[(i + 1).min(profile.len() - 1)];
        let slope = Vec2::new(next.0 - prev.0, prev.1 - next.1).normalize();
        for x in 0..=slices {
            let t = x as f32 / slices as f32 * TAU;
            mesh.vertices.push(Vertex {
                position: [t.cos() * radius, y, t.sin() * radius],
                normal: [t.cos() * slope.x, slope.y, t.sin() * slope.x],
                uv: [x as f32 / slices as f32, 1.0 - (y - profile[0].0) / height],
            });
        }
    }
    for y in 0..profile.len() as u32 - 1 {
        for x in 0..slices {
            let a = y * (slices + 1) + x;
            let b = a + slices + 1;
            mesh.indices.extend([a, b, b + 1, a, b + 1, a + 1]);
        }
    }
    if caps {
        for (index, sign) in [(0, -1.0), (profile.len() - 1, 1.0)] {
            let (y, radius) = profile[index];
            let base = mesh.vertices.len() as u32;
            mesh.vertices.push(Vertex {
                position: [0.0, y, 0.0],
                normal: [0.0, sign, 0.0],
                uv: [0.5, 0.5],
            });
            for x in 0..=slices {
                let t = x as f32 / slices as f32 * TAU;
                mesh.vertices.push(Vertex {
                    position: [t.cos() * radius, y, t.sin() * radius],
                    normal: [0.0, sign, 0.0],
                    uv: [0.5 + t.cos() * 0.5, 0.5 + t.sin() * 0.5],
                });
            }
            for x in 0..slices {
                if sign > 0.0 {
                    mesh.indices.extend([base, base + x + 2, base + x + 1]);
                } else {
                    mesh.indices.extend([base, base + x + 1, base + x + 2]);
                }
            }
        }
    }
    mesh
}

pub fn cylinder(radius: f32, height: f32, slices: u32) -> MeshData {
    let b = (height * 0.06).min(radius * 0.08);
    lathe(
        &[
            (-height / 2.0, radius - b),
            (-height / 2.0 + b, radius),
            (height / 2.0 - b, radius),
            (height / 2.0, radius - b),
        ],
        slices,
        true,
    )
}

pub fn quad(half_x: f32, half_y: f32) -> MeshData {
    MeshData {
        vertices: vec![
            Vertex {
                position: [-half_x, -half_y, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [half_x, -half_y, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [half_x, half_y, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-half_x, half_y, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

pub fn segment(from: Vec3, to: Vec3, r0: f32, r1: f32) -> MeshData {
    let d = to - from;
    let len = d.length();
    let mut out = MeshData::default();
    let transform = Mat4::from_rotation_translation(
        glam::Quat::from_rotation_arc(Vec3::Y, d.normalize()),
        (from + to) * 0.5,
    );
    out.append(
        &lathe(&[(-len * 0.5, r0), (len * 0.5, r1)], 12, true),
        transform,
    );
    out.append(&sphere(r0, 12, 8), Mat4::from_translation(from));
    out.append(&sphere(r1, 12, 8), Mat4::from_translation(to));
    out
}
