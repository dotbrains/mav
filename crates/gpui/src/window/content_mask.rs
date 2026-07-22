use core::fmt::Debug;

use crate::{Bounds, Corners, Pixels, ScaledPixels, Size};

/// Indicates which region of the window is visible. Content falling outside of this mask will not be
/// rendered.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct ContentMask<P: Clone + Debug + Default + PartialEq> {
    /// The bounds
    pub bounds: Bounds<P>,
    /// The rounded clip radii.
    pub corner_radii: Corners<P>,
}

impl<P: Clone + Debug + Default + PartialEq> ContentMask<P> {
    /// Create a rectangular content mask.
    pub fn new(bounds: Bounds<P>) -> Self {
        Self {
            bounds,
            corner_radii: Corners::default(),
        }
    }
}

impl ContentMask<Pixels> {
    /// Create a rounded content mask.
    pub fn rounded(bounds: Bounds<Pixels>, corner_radii: Corners<Pixels>) -> Self {
        let clamped_size = Size {
            width: Pixels::max(bounds.size.width, Pixels::ZERO),
            height: Pixels::max(bounds.size.height, Pixels::ZERO),
        };

        Self {
            bounds,
            corner_radii: corner_radii.clamp_radii_for_quad_size(clamped_size),
        }
    }

    /// Scale the content mask's pixel units by the given scaling factor.
    pub fn scale(&self, factor: f32) -> ContentMask<ScaledPixels> {
        ContentMask {
            bounds: self.bounds.scale(factor),
            corner_radii: self.corner_radii.scale(factor),
        }
    }

    /// Intersect the content mask with the given content mask.
    pub fn intersect(&self, other: &Self) -> Self {
        let bounds = self.bounds.intersect(&other.bounds);
        let corner_radii = Corners {
            top_left: self
                .corner_radius_for_intersection(bounds, Corner::TopLeft)
                .max(other.corner_radius_for_intersection(bounds, Corner::TopLeft)),
            top_right: self
                .corner_radius_for_intersection(bounds, Corner::TopRight)
                .max(other.corner_radius_for_intersection(bounds, Corner::TopRight)),
            bottom_right: self
                .corner_radius_for_intersection(bounds, Corner::BottomRight)
                .max(other.corner_radius_for_intersection(bounds, Corner::BottomRight)),
            bottom_left: self
                .corner_radius_for_intersection(bounds, Corner::BottomLeft)
                .max(other.corner_radius_for_intersection(bounds, Corner::BottomLeft)),
        };

        ContentMask::rounded(bounds, corner_radii)
    }

    fn corner_radius_for_intersection(&self, bounds: Bounds<Pixels>, corner: Corner) -> Pixels {
        if bounds.is_empty() {
            return Pixels::ZERO;
        }

        let self_bottom_right = self.bounds.bottom_right();
        let bounds_bottom_right = bounds.bottom_right();
        match corner {
            Corner::TopLeft
                if bounds.origin.x == self.bounds.origin.x
                    && bounds.origin.y == self.bounds.origin.y =>
            {
                self.corner_radii.top_left
            }
            Corner::TopRight
                if bounds_bottom_right.x == self_bottom_right.x
                    && bounds.origin.y == self.bounds.origin.y =>
            {
                self.corner_radii.top_right
            }
            Corner::BottomRight
                if bounds_bottom_right.x == self_bottom_right.x
                    && bounds_bottom_right.y == self_bottom_right.y =>
            {
                self.corner_radii.bottom_right
            }
            Corner::BottomLeft
                if bounds.origin.x == self.bounds.origin.x
                    && bounds_bottom_right.y == self_bottom_right.y =>
            {
                self.corner_radii.bottom_left
            }
            _ => Pixels::ZERO,
        }
    }
}

enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}
