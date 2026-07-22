pub(crate) enum GpuiMode {
    #[cfg(any(test, feature = "test-support"))]
    Test {
        skip_drawing: bool,
    },
    Production,
}
impl GpuiMode {
    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> Self {
        GpuiMode::Test {
            skip_drawing: false,
        }
    }

    #[inline]
    pub(crate) fn skip_drawing(&self) -> bool {
        match self {
            #[cfg(any(test, feature = "test-support"))]
            GpuiMode::Test { skip_drawing } => *skip_drawing,
            GpuiMode::Production => false,
        }
    }
}
