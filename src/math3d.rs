use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub};

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
        let a = self.m;
        let cofactor = [
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
        ];
        Vec3::new(
            cofactor[0][0] * normal.x + cofactor[0][1] * normal.y + cofactor[0][2] * normal.z,
            cofactor[1][0] * normal.x + cofactor[1][1] * normal.y + cofactor[1][2] * normal.z,
            cofactor[2][0] * normal.x + cofactor[2][1] * normal.y + cofactor[2][2] * normal.z,
        )
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
