use super::*;

pub enum AnyDiv {
    Div(Div),
    Stateful(Stateful<Div>),
}

impl AnyDiv {
    pub(super) fn into_any_element(self) -> AnyElement {
        match self {
            Self::Div(div) => div.into_any_element(),
            Self::Stateful(div) => div.into_any_element(),
        }
    }
}

impl From<Div> for AnyDiv {
    fn from(value: Div) -> Self {
        Self::Div(value)
    }
}

impl From<Stateful<Div>> for AnyDiv {
    fn from(value: Stateful<Div>) -> Self {
        Self::Stateful(value)
    }
}

impl Styled for AnyDiv {
    fn style(&mut self) -> &mut StyleRefinement {
        match self {
            Self::Div(div) => div.style(),
            Self::Stateful(div) => div.style(),
        }
    }
}

impl ParentElement for AnyDiv {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        match self {
            Self::Div(div) => div.extend(elements),
            Self::Stateful(div) => div.extend(elements),
        }
    }
}
