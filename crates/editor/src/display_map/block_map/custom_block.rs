use super::*;

impl CustomBlock {
    #[ztracing::instrument(skip_all)]
    pub fn render(&self, cx: &mut BlockContext) -> AnyElement {
        self.render.lock()(cx)
    }

    #[ztracing::instrument(skip_all)]
    pub fn start(&self) -> Anchor {
        *self.placement.start()
    }

    #[ztracing::instrument(skip_all)]
    pub fn end(&self) -> Anchor {
        *self.placement.end()
    }

    pub fn style(&self) -> BlockStyle {
        self.style
    }

    pub fn properties(&self) -> BlockProperties<Anchor> {
        BlockProperties {
            placement: self.placement.clone(),
            height: self.height,
            style: self.style,
            render: Arc::new(|_| {
                // Not used
                gpui::Empty.into_any_element()
            }),
            priority: self.priority,
        }
    }
}

impl Debug for CustomBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Block")
            .field("id", &self.id)
            .field("placement", &self.placement)
            .field("height", &self.height)
            .field("style", &self.style)
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}
