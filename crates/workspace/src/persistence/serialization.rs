use super::*;

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct SerializedAxis(pub(crate) gpui::Axis);
impl sqlez::bindable::StaticColumnCount for SerializedAxis {}
impl sqlez::bindable::Bind for SerializedAxis {
    fn bind(
        &self,
        statement: &sqlez::statement::Statement,
        start_index: i32,
    ) -> anyhow::Result<i32> {
        match self.0 {
            gpui::Axis::Horizontal => "Horizontal",
            gpui::Axis::Vertical => "Vertical",
        }
        .bind(statement, start_index)
    }
}

impl sqlez::bindable::Column for SerializedAxis {
    fn column(
        statement: &mut sqlez::statement::Statement,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        String::column(statement, start_index).and_then(|(axis_text, next_index)| {
            Ok((
                match axis_text.as_str() {
                    "Horizontal" => Self(Axis::Horizontal),
                    "Vertical" => Self(Axis::Vertical),
                    _ => anyhow::bail!("Stored serialized item kind is incorrect"),
                },
                next_index,
            ))
        })
    }
}

impl StaticColumnCount for PaneKind {}

impl Bind for PaneKind {
    fn bind(&self, statement: &Statement, start_index: i32) -> Result<i32> {
        let kind = match self {
            PaneKind::Tabs => "tabs",
            PaneKind::Project => "project",
            PaneKind::Agent => "agent",
        };
        kind.bind(statement, start_index)
    }
}

impl Column for PaneKind {
    fn column(statement: &mut Statement, start_index: i32) -> Result<(Self, i32)> {
        String::column(statement, start_index).and_then(|(kind, next_index)| {
            Ok((
                match kind.as_str() {
                    "tabs" => Self::Tabs,
                    "project" => Self::Project,
                    "agent" => Self::Tabs,
                    _ => Self::Tabs,
                },
                next_index,
            ))
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub(crate) struct SerializedWindowBounds(pub(crate) WindowBounds);

impl StaticColumnCount for SerializedWindowBounds {
    fn column_count() -> usize {
        5
    }
}

impl Bind for SerializedWindowBounds {
    fn bind(&self, statement: &Statement, start_index: i32) -> Result<i32> {
        match self.0 {
            WindowBounds::Windowed(bounds) => {
                let next_index = statement.bind(&"Windowed", start_index)?;
                statement.bind(
                    &(
                        SerializedPixels(bounds.origin.x),
                        SerializedPixels(bounds.origin.y),
                        SerializedPixels(bounds.size.width),
                        SerializedPixels(bounds.size.height),
                    ),
                    next_index,
                )
            }
            WindowBounds::Maximized(bounds) => {
                let next_index = statement.bind(&"Maximized", start_index)?;
                statement.bind(
                    &(
                        SerializedPixels(bounds.origin.x),
                        SerializedPixels(bounds.origin.y),
                        SerializedPixels(bounds.size.width),
                        SerializedPixels(bounds.size.height),
                    ),
                    next_index,
                )
            }
            WindowBounds::Fullscreen(bounds) => {
                let next_index = statement.bind(&"FullScreen", start_index)?;
                statement.bind(
                    &(
                        SerializedPixels(bounds.origin.x),
                        SerializedPixels(bounds.origin.y),
                        SerializedPixels(bounds.size.width),
                        SerializedPixels(bounds.size.height),
                    ),
                    next_index,
                )
            }
        }
    }
}

impl Column for SerializedWindowBounds {
    fn column(statement: &mut Statement, start_index: i32) -> Result<(Self, i32)> {
        let (window_state, next_index) = String::column(statement, start_index)?;
        let ((x, y, width, height), _): ((i32, i32, i32, i32), _) =
            Column::column(statement, next_index)?;
        let bounds = Bounds {
            origin: point(px(x as f32), px(y as f32)),
            size: size(px(width as f32), px(height as f32)),
        };

        let status = match window_state.as_str() {
            "Windowed" | "Fixed" => SerializedWindowBounds(WindowBounds::Windowed(bounds)),
            "Maximized" => SerializedWindowBounds(WindowBounds::Maximized(bounds)),
            "FullScreen" => SerializedWindowBounds(WindowBounds::Fullscreen(bounds)),
            _ => bail!("Window State did not have a valid string"),
        };

        Ok((status, next_index + 4))
    }
}

const DEFAULT_WINDOW_BOUNDS_KEY: &str = "default_window_bounds";

pub fn read_default_window_bounds(kvp: &KeyValueStore) -> Option<(Uuid, WindowBounds)> {
    let json_str = kvp
        .read_kvp(DEFAULT_WINDOW_BOUNDS_KEY)
        .log_err()
        .flatten()?;

    let (display_uuid, persisted) =
        serde_json::from_str::<(Uuid, WindowBoundsJson)>(&json_str).ok()?;
    Some((display_uuid, persisted.into()))
}

pub async fn write_default_window_bounds(
    kvp: &KeyValueStore,
    bounds: WindowBounds,
    display_uuid: Uuid,
) -> anyhow::Result<()> {
    let persisted = WindowBoundsJson::from(bounds);
    let json_str = serde_json::to_string(&(display_uuid, persisted))?;
    kvp.write_kvp(DEFAULT_WINDOW_BOUNDS_KEY.to_string(), json_str)
        .await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub enum WindowBoundsJson {
    Windowed {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    Maximized {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    Fullscreen {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
}

impl From<WindowBounds> for WindowBoundsJson {
    fn from(b: WindowBounds) -> Self {
        match b {
            WindowBounds::Windowed(bounds) => {
                let origin = bounds.origin;
                let size = bounds.size;
                WindowBoundsJson::Windowed {
                    x: f32::from(origin.x).round() as i32,
                    y: f32::from(origin.y).round() as i32,
                    width: f32::from(size.width).round() as i32,
                    height: f32::from(size.height).round() as i32,
                }
            }
            WindowBounds::Maximized(bounds) => {
                let origin = bounds.origin;
                let size = bounds.size;
                WindowBoundsJson::Maximized {
                    x: f32::from(origin.x).round() as i32,
                    y: f32::from(origin.y).round() as i32,
                    width: f32::from(size.width).round() as i32,
                    height: f32::from(size.height).round() as i32,
                }
            }
            WindowBounds::Fullscreen(bounds) => {
                let origin = bounds.origin;
                let size = bounds.size;
                WindowBoundsJson::Fullscreen {
                    x: f32::from(origin.x).round() as i32,
                    y: f32::from(origin.y).round() as i32,
                    width: f32::from(size.width).round() as i32,
                    height: f32::from(size.height).round() as i32,
                }
            }
        }
    }
}

impl From<WindowBoundsJson> for WindowBounds {
    fn from(n: WindowBoundsJson) -> Self {
        match n {
            WindowBoundsJson::Windowed {
                x,
                y,
                width,
                height,
            } => WindowBounds::Windowed(Bounds {
                origin: point(px(x as f32), px(y as f32)),
                size: size(px(width as f32), px(height as f32)),
            }),
            WindowBoundsJson::Maximized {
                x,
                y,
                width,
                height,
            } => WindowBounds::Maximized(Bounds {
                origin: point(px(x as f32), px(y as f32)),
                size: size(px(width as f32), px(height as f32)),
            }),
            WindowBoundsJson::Fullscreen {
                x,
                y,
                width,
                height,
            } => WindowBounds::Fullscreen(Bounds {
                origin: point(px(x as f32), px(y as f32)),
                size: size(px(width as f32), px(height as f32)),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SerializedPixels(gpui::Pixels);
impl sqlez::bindable::StaticColumnCount for SerializedPixels {}

impl sqlez::bindable::Bind for SerializedPixels {
    fn bind(
        &self,
        statement: &sqlez::statement::Statement,
        start_index: i32,
    ) -> anyhow::Result<i32> {
        let this: i32 = u32::from(self.0) as _;
        this.bind(statement, start_index)
    }
}
