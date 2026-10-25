//! Triangle meshes, and a few built-in solids.

use embedded_graphics_core::pixelcolor::Rgb565;

use crate::math::Vec3;

/// One triangle: three indices into [`Mesh::vertices`] and a colour.
///
/// Front faces wind **counter-clockwise** seen from outside, so with
/// [`Cull::Back`](crate::Cull::Back) only their outside is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Face {
    /// Vertex indices, counter-clockwise from the front.
    pub indices: [u16; 3],
    /// Base colour, before lighting and fog.
    pub color: Rgb565,
}

impl Face {
    /// A face over vertices `a`, `b`, `c`.
    pub const fn new(a: u16, b: u16, c: u16, color: Rgb565) -> Self {
        Self {
            indices: [a, b, c],
            color,
        }
    }
}

/// An indexed triangle mesh, borrowed from (usually `static`) arrays.
///
/// [`Mesh::new`] is `const`, checks every index, and works out a bounding
/// sphere about the model origin that the renderer uses to skip whole
/// meshes outside the view. A `const` mesh therefore costs only its
/// arrays, in flash, and a bad index is a compile error:
///
/// ```
/// use oxstack_3d::math::Vec3;
/// use oxstack_3d::mesh::{Face, Mesh};
/// use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
///
/// const TRIANGLE: Mesh<'static> = Mesh::new(
///     &[Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)],
///     &[Face::new(0, 1, 2, Rgb565::YELLOW)],
/// );
/// assert_eq!(TRIANGLE.bounding_radius_squared(), 1.0);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Mesh<'a> {
    vertices: &'a [Vec3],
    faces: &'a [Face],
    radius_squared: f32,
}

impl<'a> Mesh<'a> {
    /// A mesh over `vertices` and `faces`.
    ///
    /// # Panics
    ///
    /// If a face indexes past `vertices` (at compile time in a `const`).
    pub const fn new(vertices: &'a [Vec3], faces: &'a [Face]) -> Self {
        let mut i = 0;
        while i < faces.len() {
            let [a, b, c] = faces[i].indices;
            let n = vertices.len();
            assert!(
                (a as usize) < n && (b as usize) < n && (c as usize) < n,
                "face index out of range"
            );
            i += 1;
        }
        let mut radius_squared = 0.0;
        let mut i = 0;
        while i < vertices.len() {
            let v = vertices[i];
            let d = v.x * v.x + v.y * v.y + v.z * v.z;
            if d > radius_squared {
                radius_squared = d;
            }
            i += 1;
        }
        Self {
            vertices,
            faces,
            radius_squared,
        }
    }

    /// Vertex positions in model space.
    pub const fn vertices(&self) -> &'a [Vec3] {
        self.vertices
    }

    /// Triangles.
    pub const fn faces(&self) -> &'a [Face] {
        self.faces
    }

    /// Squared radius of a sphere about the model origin holding every
    /// vertex.
    pub const fn bounding_radius_squared(&self) -> f32 {
        self.radius_squared
    }
}

/// A palette for the built-in shapes, chosen to stay distinct after
/// flat shading on an RGB565 panel.
const PALETTE: [Rgb565; 6] = [
    Rgb565::new(31, 22, 6),  // amber
    Rgb565::new(6, 40, 30),  // teal
    Rgb565::new(28, 10, 12), // rose
    Rgb565::new(10, 30, 31), // sky
    Rgb565::new(20, 50, 8),  // lime
    Rgb565::new(22, 16, 31), // lavender
];

const fn c(i: usize) -> Rgb565 {
    PALETTE[i % PALETTE.len()]
}

/// A cube from −1 to 1 on each axis: 8 vertices, 12 triangles, one
/// colour per side.
pub const CUBE: Mesh<'static> = Mesh::new(
    &[
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
    ],
    &[
        Face::new(4, 5, 6, c(0)), // +Z
        Face::new(4, 6, 7, c(0)),
        Face::new(1, 0, 3, c(1)), // −Z
        Face::new(1, 3, 2, c(1)),
        Face::new(5, 1, 2, c(2)), // +X
        Face::new(5, 2, 6, c(2)),
        Face::new(0, 4, 7, c(3)), // −X
        Face::new(0, 7, 3, c(3)),
        Face::new(7, 6, 2, c(4)), // +Y
        Face::new(7, 2, 3, c(4)),
        Face::new(0, 1, 5, c(5)), // −Y
        Face::new(0, 5, 4, c(5)),
    ],
);

/// A regular octahedron with its vertices on the axes at distance 1.
pub const OCTAHEDRON: Mesh<'static> = Mesh::new(
    &[
        Vec3::X,
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::Z,
        Vec3::new(0.0, 0.0, -1.0),
    ],
    &[
        Face::new(0, 2, 4, c(0)),
        Face::new(1, 4, 2, c(1)),
        Face::new(0, 4, 3, c(2)),
        Face::new(0, 5, 2, c(3)),
        Face::new(1, 3, 4, c(4)),
        Face::new(1, 2, 5, c(5)),
        Face::new(0, 3, 5, c(1)),
        Face::new(1, 5, 3, c(0)),
    ],
);

// Icosahedron vertices (0, ±1, ±φ) and cyclic permutations, scaled to
// radius 1: A = 1/√(1+φ²), B = φ/√(1+φ²).
const A: f32 = 0.525_731_1;
const B: f32 = 0.850_650_8;

/// A regular icosahedron of radius 1: 12 vertices, 20 triangles. A good
/// stand-in for a sphere.
pub const ICOSAHEDRON: Mesh<'static> = Mesh::new(
    &[
        Vec3::new(-A, B, 0.0),
        Vec3::new(A, B, 0.0),
        Vec3::new(-A, -B, 0.0),
        Vec3::new(A, -B, 0.0),
        Vec3::new(0.0, -A, B),
        Vec3::new(0.0, A, B),
        Vec3::new(0.0, -A, -B),
        Vec3::new(0.0, A, -B),
        Vec3::new(B, 0.0, -A),
        Vec3::new(B, 0.0, A),
        Vec3::new(-B, 0.0, -A),
        Vec3::new(-B, 0.0, A),
    ],
    &[
        Face::new(0, 11, 5, c(0)),
        Face::new(0, 5, 1, c(1)),
        Face::new(0, 1, 7, c(2)),
        Face::new(0, 7, 10, c(3)),
        Face::new(0, 10, 11, c(4)),
        Face::new(1, 5, 9, c(5)),
        Face::new(5, 11, 4, c(2)),
        Face::new(11, 10, 2, c(1)),
        Face::new(10, 7, 6, c(0)),
        Face::new(7, 1, 8, c(4)),
        Face::new(3, 9, 4, c(3)),
        Face::new(3, 4, 2, c(5)),
        Face::new(3, 2, 6, c(4)),
        Face::new(3, 6, 8, c(1)),
        Face::new(3, 8, 9, c(0)),
        Face::new(4, 9, 5, c(4)),
        Face::new(2, 4, 11, c(0)),
        Face::new(6, 2, 10, c(5)),
        Face::new(8, 6, 7, c(3)),
        Face::new(9, 8, 1, c(2)),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_solids_wind_counter_clockwise_outward() {
        for (name, mesh) in [
            ("cube", CUBE),
            ("octahedron", OCTAHEDRON),
            ("icosahedron", ICOSAHEDRON),
        ] {
            for f in mesh.faces() {
                let [a, b, c] = f.indices.map(|i| mesh.vertices()[i as usize]);
                let n = (b - a).cross(c - a);
                let centroid = (a + b + c) * (1.0 / 3.0);
                assert!(n.dot(centroid) > 0.0, "{name}: face {f:?} points inward");
            }
            // Closed: every edge appears once in each direction.
            for f in mesh.faces() {
                for k in 0..3 {
                    let (a, b) = (f.indices[k], f.indices[(k + 1) % 3]);
                    let twins = mesh
                        .faces()
                        .iter()
                        .flat_map(|g| (0..3).map(move |j| (g.indices[j], g.indices[(j + 1) % 3])))
                        .filter(|&e| e == (b, a))
                        .count();
                    assert_eq!(twins, 1, "{name}: edge {a}-{b}");
                }
            }
        }
        assert_eq!(CUBE.bounding_radius_squared(), 3.0);
        assert!((ICOSAHEDRON.bounding_radius_squared() - 1.0).abs() < 1e-6);
    }
}
