use std::ops;
use std::ops::{AddAssign, Add};

#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Point<N> {
    pub x: N,
    pub y: N
}

impl<N> Point<N> {
    /// Construct a point from its coordinates. Mirrors the signature of
    /// `speedy2d::dimen::Vector2::new` so mouse-position call sites read the same.
    #[inline]
    pub const fn new(x: N, y: N) -> Self {
        Point { x, y }
    }
}

impl<T> From<(T, T)> for Point<T> where T: Copy {
    #[inline]
    fn from(value: (T, T)) -> Self {
        point(value.0, value.1)
    }
}

impl<N: Copy + std::ops::Add<Output = N>> AddAssign for Point<N> {
    fn add_assign(&mut self, rhs: Self) {
        self.x = self.x + rhs.x;
        self.y = self.y + rhs.y;
    }
}

impl<N: Copy + std::ops::Add<Output = N>> Add for Point<N> {
    type Output = Point<N>;

    fn add(self, rhs: Self) -> Self::Output {
        Point::from((self.x + rhs.x, self.y + rhs.y))
    }
}

/// A convenience function for generating `Point`s.
#[inline]
pub fn point<N>(x: N, y: N) -> Point<N> {
    Point { x, y }
}

#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Rect<N> {
    pub min: Point<N>,
    pub max: Point<N>
}

/// A convenience function for generating `Rect`s.
#[inline]
pub fn rect<N, P: Into<Point<N>>>(min: P, max: P) -> Rect<N> {
    Rect { min: min.into(), max: max.into() }
}

impl<N: ops::Sub<Output = N> + Copy + Add<Output = N> + PartialOrd + AddAssign + ops::SubAssign> Rect<N> {
    pub fn width(&self) -> N {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> N {
        self.max.y - self.min.y
    }

    pub fn move_to<P: Into<Point<N>>>(&mut self, min: P) {
        let min = min.into();
        let delta = point(min.x - self.min.x, min.y - self.min.y);
        self.min += delta.clone();
        self.max += delta;
    }

    pub fn move_by<P: Into<Point<N>>>(&mut self, delta: P) {
        let delta = delta.into();
        self.min.x = self.min.x + delta.x;
        self.max.x = self.max.x + delta.x;
        self.min.y = self.min.y + delta.y;
        self.max.y = self.max.y + delta.y;
    }

    pub fn shrink_by(&mut self, top: N, left: N, right: N, bottom: N) {
        self.min.y += top;
        self.min.x += left;
        self.max.x -= right;
        self.max.y -= bottom;
    }

    pub fn clear(&mut self) {
        self.min.x -= self.min.x;
        self.min.y -= self.min.y;
        self.max.x -= self.max.x;
        self.max.y -= self.max.y;
    }

    pub fn hit<P: Into<Point<N>>>(&self, point: P) -> bool {
        let point = point.into();
        self.min.x <= point.x && self.max.x >= point.x && self.min.y <= point.y && self.max.y >= point.y
    }
}