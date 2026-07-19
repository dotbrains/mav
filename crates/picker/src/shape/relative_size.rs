use gpui::{Pixels, Rems, Window};

macro_rules! relative_size {
    ($name:ident, $accessor:ident) => {
        /// Size type that is the sum of a relative size to the viewport and a
        /// size relative to the font size (Rems). You can
        /// add/subtract/multiple/divide to your harts content but once you
        /// need a single unit you must provide a window to get it.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct $name {
            viewport_fraction: f32,
            rems: Rems,
        }

        impl From<Rems> for $name {
            fn from(v: Rems) -> Self {
                Self::rems(v)
            }
        }

        impl $name {
            pub const FULL: Self = Self {
                viewport_fraction: 1.0,
                rems: Rems::ZERO,
            };

            pub const fn viewport(fraction: f32) -> Self {
                debug_assert!(fraction <= 1.0);
                debug_assert!(fraction >= 0.0);
                Self {
                    viewport_fraction: fraction.clamp(0.0, 1.0),
                    rems: Rems::ZERO,
                }
            }

            pub const fn rems(val: Rems) -> Self {
                Self {
                    viewport_fraction: 0.0,
                    rems: val,
                }
            }

            pub fn as_pixels(&self, window: &Window) -> Pixels {
                self.viewport_fraction * window.viewport_size().$accessor
                    + self.rems * window.rem_size()
            }

            pub fn from_pixels(width: Pixels, window: &Window) -> Self {
                Self {
                    viewport_fraction: width / window.viewport_size().$accessor,
                    rems: Rems::ZERO,
                }
            }

            /// Returns this size as [`Rems`] when it has no viewport-relative
            /// component. Used to derive a rems-based minimum from an initial
            /// size without needing a [`Window`].
            pub fn as_rems(&self) -> Option<Rems> {
                (self.viewport_fraction == 0.0).then_some(self.rems)
            }

            pub fn as_viewport_fraction(&self, window: &Window) -> ViewportFraction {
                ViewportFraction(
                    self.viewport_fraction
                        + self.rems * window.rem_size() / window.viewport_size().$accessor,
                )
            }
        }

        impl std::ops::Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self {
                    viewport_fraction: self.viewport_fraction + rhs.viewport_fraction,
                    rems: self.rems + rhs.rems,
                }
            }
        }

        impl std::ops::Sub for $name {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                Self {
                    viewport_fraction: self.viewport_fraction - rhs.viewport_fraction,
                    rems: self.rems - rhs.rems,
                }
            }
        }

        impl std::ops::Sub<Rems> for $name {
            type Output = Self;

            fn sub(self, rhs: Rems) -> Self::Output {
                Self {
                    viewport_fraction: self.viewport_fraction,
                    rems: self.rems - rhs,
                }
            }
        }

        impl std::ops::Div<f32> for $name {
            type Output = Self;

            fn div(mut self, rhs: f32) -> Self::Output {
                self.viewport_fraction /= rhs;
                self.rems = Rems(self.rems.0 / rhs);
                self
            }
        }

        impl std::ops::Mul<f32> for $name {
            type Output = Self;

            fn mul(mut self, rhs: f32) -> Self::Output {
                self.viewport_fraction *= rhs;
                self.rems = Rems(self.rems.0 * rhs);
                self
            }
        }
    };
}

relative_size!(RelativeHeight, height);
relative_size!(RelativeWidth, width);

#[derive(Debug, Clone, Copy)]
pub struct ViewportFraction(f32);

impl ViewportFraction {
    pub(crate) const ZERO: Self = Self(0.0);
    pub(crate) fn fraction(v: f32) -> Self {
        debug_assert!(
            (0.0..=1.0).contains(&v),
            "ViewportFraction must be between zero and one"
        );
        Self(v)
    }

    pub(crate) fn width_as_pixels(&self, window: &Window) -> Pixels {
        window.viewport_size().width * self.0
    }
    pub(crate) fn height_as_pixels(&self, window: &Window) -> Pixels {
        window.viewport_size().height * self.0
    }

    pub(crate) fn from_height_pixels(preview: Pixels, window: &Window) -> Self {
        Self(preview / window.viewport_size().height)
    }

    pub(crate) fn from_width_pixels(preview: Pixels, window: &Window) -> Self {
        Self(preview / window.viewport_size().width)
    }

    /// Returns the fraction of the viewport that this describes.
    /// Guaranteed to be between zero and one
    pub(crate) fn raw(&self) -> f32 {
        self.0
    }
}

impl std::ops::Mul<f32> for ViewportFraction {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self(self.0 * rhs)
    }
}
