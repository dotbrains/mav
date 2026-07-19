use crate::{
    DevicePixels, ForegroundExecutor, ScreenCaptureFrame, ScreenCaptureSource, ScreenCaptureStream,
    SourceMetadata, size,
};
use anyhow::Result;
use futures::channel::oneshot;

#[derive(Clone)]
/// A fake screen capture source, used for testing.
pub struct TestScreenCaptureSource {}

/// A fake screen capture stream, used for testing.
pub struct TestScreenCaptureStream {}

impl ScreenCaptureSource for TestScreenCaptureSource {
    fn metadata(&self) -> Result<SourceMetadata> {
        Ok(SourceMetadata {
            id: 0,
            is_main: None,
            label: None,
            resolution: size(DevicePixels(1), DevicePixels(1)),
        })
    }

    fn stream(
        &self,
        _foreground_executor: &ForegroundExecutor,
        _frame_callback: Box<dyn Fn(ScreenCaptureFrame) + Send>,
    ) -> oneshot::Receiver<Result<Box<dyn ScreenCaptureStream>>> {
        let (mut tx, rx) = oneshot::channel();
        let stream = TestScreenCaptureStream {};
        tx.send(Ok(Box::new(stream) as Box<dyn ScreenCaptureStream>))
            .ok();
        rx
    }
}

impl ScreenCaptureStream for TestScreenCaptureStream {
    fn metadata(&self) -> Result<SourceMetadata> {
        TestScreenCaptureSource {}.metadata()
    }
}

impl TestScreenCaptureSource {
    /// Create a fake screen capture source, for testing.
    pub fn new() -> Self {
        Self {}
    }
}
