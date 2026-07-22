use super::*;

impl<T: PartialOrd + Add<T, Output = T> + Sub<Output = T> + Clone + Debug + Default + PartialEq>
    Bounds<T>
{
    /// Calculates the intersection of two `Bounds` objects.
    ///
    /// This method computes the overlapping region of two `Bounds`. If the bounds do not intersect,
    /// the resulting `Bounds` will have a size with width and height of zero.
    ///
    /// # Arguments
    ///
    /// * `other` - A reference to another `Bounds` to intersect with.
    ///
    /// # Returns
    ///
    /// Returns a `Bounds` representing the intersection area. If there is no intersection,
    /// the returned `Bounds` will have a size with width and height of zero.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gpui::{Bounds, Point, Size};
    /// let bounds1 = Bounds {
    ///     origin: Point { x: 0, y: 0 },
    ///     size: Size { width: 10, height: 10 },
    /// };
    /// let bounds2 = Bounds {
    ///     origin: Point { x: 5, y: 5 },
    ///     size: Size { width: 10, height: 10 },
    /// };
    /// let intersection = bounds1.intersect(&bounds2);
    ///
    /// assert_eq!(intersection, Bounds {
    ///     origin: Point { x: 5, y: 5 },
    ///     size: Size { width: 5, height: 5 },
    /// });
    /// ```
    pub fn intersect(&self, other: &Self) -> Self {
        let upper_left = self.origin.max(&other.origin);
        let bottom_right = self
            .bottom_right()
            .min(&other.bottom_right())
            .max(&upper_left);
        Self::from_corners(upper_left, bottom_right)
    }

    /// Computes the union of two `Bounds`.
    ///
    /// This method calculates the smallest `Bounds` that contains both the current `Bounds` and the `other` `Bounds`.
    /// The resulting `Bounds` will have an origin that is the minimum of the origins of the two `Bounds`,
    /// and a size that encompasses the furthest extents of both `Bounds`.
    ///
    /// # Arguments
    ///
    /// * `other` - A reference to another `Bounds` to create a union with.
    ///
    /// # Returns
    ///
    /// Returns a `Bounds` representing the union of the two `Bounds`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gpui::{Bounds, Point, Size};
    /// let bounds1 = Bounds {
    ///     origin: Point { x: 0, y: 0 },
    ///     size: Size { width: 10, height: 10 },
    /// };
    /// let bounds2 = Bounds {
    ///     origin: Point { x: 5, y: 5 },
    ///     size: Size { width: 15, height: 15 },
    /// };
    /// let union_bounds = bounds1.union(&bounds2);
    ///
    /// assert_eq!(union_bounds, Bounds {
    ///     origin: Point { x: 0, y: 0 },
    ///     size: Size { width: 20, height: 20 },
    /// });
    /// ```
    pub fn union(&self, other: &Self) -> Self {
        let top_left = self.origin.min(&other.origin);
        let bottom_right = self.bottom_right().max(&other.bottom_right());
        Bounds::from_corners(top_left, bottom_right)
    }
}

impl<T> Bounds<T>
where
    T: Add<T, Output = T> + Sub<T, Output = T> + Clone + Debug + Default + PartialEq,
{
    /// Computes the space available within outer bounds.
    pub fn space_within(&self, outer: &Self) -> Edges<T> {
        Edges {
            top: self.top() - outer.top(),
            right: outer.right() - self.right(),
            bottom: outer.bottom() - self.bottom(),
            left: self.left() - outer.left(),
        }
    }
}

impl<T, Rhs> Mul<Rhs> for Bounds<T>
where
    T: Mul<Rhs, Output = Rhs> + Clone + Debug + Default + PartialEq,
    Point<T>: Mul<Rhs, Output = Point<Rhs>>,
    Rhs: Clone + Debug + Default + PartialEq,
{
    type Output = Bounds<Rhs>;

    fn mul(self, rhs: Rhs) -> Self::Output {
        Bounds {
            origin: self.origin * rhs.clone(),
            size: self.size * rhs,
        }
    }
}

impl<T, S> MulAssign<S> for Bounds<T>
where
    T: Mul<S, Output = T> + Clone + Debug + Default + PartialEq,
    S: Clone,
{
    fn mul_assign(&mut self, rhs: S) {
        self.origin *= rhs.clone();
        self.size *= rhs;
    }
}

impl<T, S> Div<S> for Bounds<T>
where
    Size<T>: Div<S, Output = Size<T>>,
    T: Div<S, Output = T> + Clone + Debug + Default + PartialEq,
    S: Clone,
{
    type Output = Self;

    fn div(self, rhs: S) -> Self {
        Self {
            origin: self.origin / rhs.clone(),
            size: self.size / rhs,
        }
    }
}

impl<T> Add<Point<T>> for Bounds<T>
where
    T: Add<T, Output = T> + Clone + Debug + Default + PartialEq,
{
    type Output = Self;

    fn add(self, rhs: Point<T>) -> Self {
        Self {
            origin: self.origin + rhs,
            size: self.size,
        }
    }
}

impl<T> Sub<Point<T>> for Bounds<T>
where
    T: Sub<T, Output = T> + Clone + Debug + Default + PartialEq,
{
    type Output = Self;

    fn sub(self, rhs: Point<T>) -> Self {
        Self {
            origin: self.origin - rhs,
            size: self.size,
        }
    }
}
