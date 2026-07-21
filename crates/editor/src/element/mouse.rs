mod buttons;
mod drag;
mod hover;
mod listeners;

use std::ops::Range;
use std::time::{Duration, Instant};

use collections::HashMap;
use feature_flags::{DiffReviewFeatureFlag, FeatureFlagAppExt as _};
use gpui::{
    AnyElement, App, AvailableSpace, ClickEvent, Context, DefiniteLength, DispatchPhase, Element,
    MouseButton, MouseClickEvent, MouseDownEvent, MouseMoveEvent, MousePressureEvent, MouseUpEvent,
    ParentElement, Pixels, PressureStage, ScrollDelta, ScrollWheelEvent, TextStyleRefinement,
    Window, anchored, deferred, point, px,
};
use multi_buffer::MultiBufferRow;
use project::DisableAiSettings;
use settings::Settings;
use sum_tree::Bias;
use text::SelectionGoal;
use theme_settings::BufferLineHeight;
use util::{RangeExt, debug_panic, post_inc};

use super::{EditorElement, EditorLayout, LineNumberLayout, PositionMap, SplitSide};
use crate::{
    CURSORS_VISIBLE_FOR, ColumnarMode, DisplayDiffHunk, DisplayPoint, DisplayRow, Editor,
    EditorSettings, EditorSnapshot, GutterHoverButton, HoveredCursor, JumpData,
    PhantomDiffReviewIndicator, SelectPhase, Selection, SelectionDragState,
    display_map::ToDisplayPoint, editor_settings::DoubleClickInMultibuffer,
    hover_popover::hover_at, mouse_context_menu, scroll::ScrollPixelOffset,
};
