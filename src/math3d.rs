use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

pub const PI: f32 = std::f32::consts::PI;

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length > 1.0e-7 {
            self / length
        } else {
            Self::ZERO
        }
    }
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
    pub fn mul_elem(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
    pub fn distance(self, rhs: Self) -> f32 {
        (self - rhs).length()
    }
    /// Componente de `self` perpendicular a `axis` (normalizado).
    pub fn reject(self, axis: Self) -> Self {
        self - axis * self.dot(axis)
    }
    /// Rota el vector alrededor de un eje unitario (fórmula de Rodrigues).
    pub fn rotate_around(self, axis: Self, radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        self * c + axis.cross(self) * s + axis * axis.dot(self) * (1.0 - c)
    }
    /// Devuelve un vector unitario perpendicular cualquiera.
    pub fn any_perpendicular(self) -> Self {
        let helper = if self.y.abs() < 0.9 {
            Self::new(0.0, 1.0, 0.0)
        } else {
            Self::new(1.0, 0.0, 0.0)
        };
        self.cross(helper).normalized()
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}
impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}
impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Mat4 {
    pub const fn identity() -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn translation(v: Vec3) -> Self {
        let mut result = Self::identity();
        result.m[0][3] = v.x;
        result.m[1][3] = v.y;
        result.m[2][3] = v.z;
        result
    }

    pub fn scale(v: Vec3) -> Self {
        let mut result = Self { m: [[0.0; 4]; 4] };
        result.m[0][0] = v.x;
        result.m[1][1] = v.y;
        result.m[2][2] = v.z;
        result.m[3][3] = 1.0;
        result
    }

    pub fn rotation_x(radians: f32) -> Self {
        let mut result = Self::identity();
        let (s, c) = radians.sin_cos();
        result.m[1][1] = c;
        result.m[1][2] = -s;
        result.m[2][1] = s;
        result.m[2][2] = c;
        result
    }

    pub fn rotation_y(radians: f32) -> Self {
        let mut result = Self::identity();
        let (s, c) = radians.sin_cos();
        result.m[0][0] = c;
        result.m[0][2] = s;
        result.m[2][0] = -s;
        result.m[2][2] = c;
        result
    }

    pub fn rotation_z(radians: f32) -> Self {
        let mut result = Self::identity();
        let (s, c) = radians.sin_cos();
        result.m[0][0] = c;
        result.m[0][1] = -s;
        result.m[1][0] = s;
        result.m[1][1] = c;
        result
    }

    /// Rotación alrededor de un eje unitario arbitrario.
    pub fn rotation_axis(axis: Vec3, radians: f32) -> Self {
        let a = axis.normalized();
        let (s, c) = radians.sin_cos();
        let t = 1.0 - c;
        Self {
            m: [
                [
                    t * a.x * a.x + c,
                    t * a.x * a.y - s * a.z,
                    t * a.x * a.z + s * a.y,
                    0.0,
                ],
                [
                    t * a.x * a.y + s * a.z,
                    t * a.y * a.y + c,
                    t * a.y * a.z - s * a.x,
                    0.0,
                ],
                [
                    t * a.x * a.z - s * a.y,
                    t * a.y * a.z + s * a.x,
                    t * a.z * a.z + c,
                    0.0,
                ],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Matriz cuyas columnas son los ejes locales X, Y, Z y el origen.
    pub fn from_basis(x: Vec3, y: Vec3, z: Vec3, origin: Vec3) -> Self {
        Self {
            m: [
                [x.x, y.x, z.x, origin.x],
                [x.y, y.y, z.y, origin.y],
                [x.z, y.z, z.z, origin.z],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Escala uniforme.
    pub fn uniform_scale(value: f32) -> Self {
        Self::scale(Vec3::new(value, value, value))
    }

    /// Origen (traslación) de la matriz.
    pub fn origin(self) -> Vec3 {
        Vec3::new(self.m[0][3], self.m[1][3], self.m[2][3])
    }

    /// Mayor factor de escala de la parte lineal; sirve para escalar esferas
    /// envolventes en el descarte por frustum.
    pub fn max_scale(self) -> f32 {
        let column = |c: usize| Vec3::new(self.m[0][c], self.m[1][c], self.m[2][c]).length();
        column(0).max(column(1)).max(column(2))
    }

    /// Inversa de una transformación rígida con escala uniforme
    /// (rotación * escala + traslación).
    pub fn inverse_similarity(self) -> Self {
        let scale_sq = Vec3::new(self.m[0][0], self.m[1][0], self.m[2][0]).length_squared();
        let inv = if scale_sq > 1.0e-12 {
            1.0 / scale_sq
        } else {
            0.0
        };
        let mut result = Self::identity();
        for r in 0..3 {
            for c in 0..3 {
                result.m[r][c] = self.m[c][r] * inv;
            }
        }
        let t = self.origin();
        let moved = result.transform_direction(t);
        result.m[0][3] = -moved.x;
        result.m[1][3] = -moved.y;
        result.m[2][3] = -moved.z;
        result
    }

    /// Matriz 3x3 para normales (cofactores = inversa transpuesta * det).
    pub fn normal_matrix(self) -> [[f32; 3]; 3] {
        let a = self.m;
        [
            [
                a[1][1] * a[2][2] - a[1][2] * a[2][1],
                -(a[1][0] * a[2][2] - a[1][2] * a[2][0]),
                a[1][0] * a[2][1] - a[1][1] * a[2][0],
            ],
            [
                -(a[0][1] * a[2][2] - a[0][2] * a[2][1]),
                a[0][0] * a[2][2] - a[0][2] * a[2][0],
                -(a[0][0] * a[2][1] - a[0][1] * a[2][0]),
            ],
            [
                a[0][1] * a[1][2] - a[0][2] * a[1][1],
                -(a[0][0] * a[1][2] - a[0][2] * a[1][0]),
                a[0][0] * a[1][1] - a[0][1] * a[1][0],
            ],
        ]
    }

    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let mut result = Self { m: [[0.0; 4]; 4] };
        let f = 1.0 / (fov_y * 0.5).tan();
        result.m[0][0] = f / aspect;
        result.m[1][1] = f;
        result.m[2][2] = (far + near) / (near - far);
        result.m[2][3] = 2.0 * far * near / (near - far);
        result.m[3][2] = -1.0;
        result
    }

    pub fn look_at(eye: Vec3, target: Vec3, world_up: Vec3) -> Self {
        let forward = (target - eye).normalized();
        let right = forward.cross(world_up).normalized();
        let up = right.cross(forward);
        Self {
            m: [
                [right.x, right.y, right.z, -right.dot(eye)],
                [up.x, up.y, up.z, -up.dot(eye)],
                [-forward.x, -forward.y, -forward.z, forward.dot(eye)],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn transform_point(self, point: Vec3) -> Vec3 {
        let value = self * Vec4::new(point.x, point.y, point.z, 1.0);
        Vec3::new(value.x, value.y, value.z)
    }

    pub fn transform_direction(self, direction: Vec3) -> Vec3 {
        let value = self * Vec4::new(direction.x, direction.y, direction.z, 0.0);
        Vec3::new(value.x, value.y, value.z)
    }

    /// Transforma una normal con la inversa transpuesta de la matriz 3x3.
    /// Esto mantiene correcta la iluminación cuando un modelo usa escala no uniforme.
    pub fn transform_normal(self, normal: Vec3) -> Vec3 {
        apply3(&self.normal_matrix(), normal)
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut result = Self { m: [[0.0; 4]; 4] };
        for row in 0..4 {
            for column in 0..4 {
                for k in 0..4 {
                    result.m[row][column] += self.m[row][k] * rhs.m[k][column];
                }
            }
        }
        result
    }
}

impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    fn mul(self, v: Vec4) -> Vec4 {
        Vec4::new(
            self.m[0][0] * v.x + self.m[0][1] * v.y + self.m[0][2] * v.z + self.m[0][3] * v.w,
            self.m[1][0] * v.x + self.m[1][1] * v.y + self.m[1][2] * v.z + self.m[1][3] * v.w,
            self.m[2][0] * v.x + self.m[2][1] * v.y + self.m[2][2] * v.z + self.m[2][3] * v.w,
            self.m[3][0] * v.x + self.m[3][1] * v.y + self.m[3][2] * v.z + self.m[3][3] * v.w,
        )
    }
}

/// Multiplica una matriz 3x3 por un vector.
#[inline]
pub fn apply3(m: &[[f32; 3]; 3], v: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z,
        m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z,
        m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z,
    )
}

/// Interpolación suave tipo Hermite entre 0 y 1.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Factor de suavizado exponencial independiente de la tasa de cuadros.
pub fn damp_factor(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

/// Generador pseudoaleatorio reproducible (xorshift32) usado por toda la
/// generación procedural: la misma semilla produce siempre el mismo mundo.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9).wrapping_add(0x7F4A_7C15) | 1,
        }
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
    /// Número uniforme en [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }
    pub fn index(&mut self, len: usize) -> usize {
        (self.next_u32() as usize) % len.max(1)
    }
    /// Dirección uniforme sobre la esfera unitaria.
    pub fn unit_vector(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let angle = self.range(0.0, 2.0 * PI);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * angle.cos(), z, r * angle.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1.0e-4
    }

    #[test]
    fn rotation_y_turns_x_toward_negative_z() {
        let r = Mat4::rotation_y(PI * 0.5).transform_direction(Vec3::new(1.0, 0.0, 0.0));
        assert!(close(r, Vec3::new(0.0, 0.0, -1.0)));
    }

    #[test]
    fn axis_rotation_matches_rodrigues() {
        let axis = Vec3::new(1.0, 2.0, 3.0).normalized();
        let v = Vec3::new(0.3, -1.0, 2.0);
        let a = Mat4::rotation_axis(axis, 0.7).transform_direction(v);
        let b = v.rotate_around(axis, 0.7);
        assert!(close(a, b));
    }

    #[test]
    fn similarity_inverse_round_trips() {
        let m = Mat4::translation(Vec3::new(3.0, -2.0, 5.0))
            * Mat4::rotation_axis(Vec3::new(0.2, 1.0, 0.1).normalized(), 1.1)
            * Mat4::uniform_scale(2.5);
        let p = Vec3::new(1.0, 2.0, 3.0);
        let back = m.inverse_similarity().transform_point(m.transform_point(p));
        assert!(close(back, p));
    }

    #[test]
    fn look_at_places_target_in_front_of_camera() {
        let view = Mat4::look_at(
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
        );
        let p = view.transform_point(Vec3::ZERO);
        assert!(close(p, Vec3::new(0.0, 0.0, -10.0)));
    }

    #[test]
    fn perspective_maps_near_and_far_to_ndc_limits() {
        let proj = Mat4::perspective(1.0, 1.0, 0.5, 100.0);
        let near = proj * Vec4::new(0.0, 0.0, -0.5, 1.0);
        let far = proj * Vec4::new(0.0, 0.0, -100.0, 1.0);
        assert!((near.z / near.w + 1.0).abs() < 1.0e-4);
        assert!((far.z / far.w - 1.0).abs() < 1.0e-3);
    }

    #[test]
    fn normal_matrix_handles_non_uniform_scale() {
        let m = Mat4::scale(Vec3::new(4.0, 1.0, 1.0));
        // Superficie inclinada 45° en XY: tras escalar X, la normal se inclina hacia X menos.
        let n = m
            .transform_normal(Vec3::new(1.0, 1.0, 0.0).normalized())
            .normalized();
        assert!(n.y > n.x);
    }

    #[test]
    fn rng_is_reproducible() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        let v = Rng::new(7).unit_vector();
        assert!((v.length() - 1.0).abs() < 1.0e-4);
    }
}
