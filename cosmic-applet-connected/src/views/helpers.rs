//! Helper functions and constants for view rendering.

use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Color, Length};
use cosmic::iced_core::layout::Limits;
use cosmic::iced_core::Shadow;
use cosmic::widget::autosize::autosize;
use cosmic::widget::container::Container;
use cosmic::{Element, Renderer};
use cosmic_panel_config::PanelAnchor;

/// Default popup width (matches libcosmic default).
pub const DEFAULT_POPUP_WIDTH: f32 = 360.0;

/// Wide popup width for SMS/media views that need more space.
pub const WIDE_POPUP_WIDTH: f32 = 450.0;

/// Maximum height of the popup window in pixels.
pub const POPUP_MAX_HEIGHT: f32 = 1000.0;

/// ID for the autosize widget used in popup container.
static POPUP_AUTOSIZE_ID: std::sync::LazyLock<cosmic::widget::Id> =
    std::sync::LazyLock::new(|| cosmic::widget::Id::new("popup-autosize"));

/// Create a popup container with specified width.
/// Based on libcosmic's popup_container but with configurable width limits.
pub fn popup_container<'a, Message: 'a + 'static>(
    content: impl Into<Element<'a, Message>>,
    width: f32,
    anchor: PanelAnchor,
) -> Element<'a, Message> {
    let (vertical_align, horizontal_align) = match anchor {
        PanelAnchor::Left => (Vertical::Center, Horizontal::Left),
        PanelAnchor::Right => (Vertical::Center, Horizontal::Right),
        PanelAnchor::Top => (Vertical::Top, Horizontal::Center),
        PanelAnchor::Bottom => (Vertical::Bottom, Horizontal::Center),
    };

    autosize(
        Container::<Message, cosmic::Theme, Renderer>::new(
            Container::<Message, cosmic::Theme, Renderer>::new(content).style(|theme| {
                let cosmic = theme.cosmic();
                let corners = cosmic.corner_radii;
                cosmic::iced_widget::container::Style {
                    text_color: Some(cosmic.background.on.into()),
                    background: Some(Color::from(cosmic.background.base).into()),
                    border: cosmic::iced::Border {
                        radius: corners.radius_m.into(),
                        width: 1.0,
                        color: cosmic.background.divider.into(),
                    },
                    shadow: Shadow::default(),
                    icon_color: Some(cosmic.background.on.into()),
                }
            }),
        )
        .width(Length::Shrink)
        .height(Length::Shrink)
        .align_x(horizontal_align)
        .align_y(vertical_align),
        POPUP_AUTOSIZE_ID.clone(),
    )
    .limits(
        Limits::NONE
            .min_height(1.0)
            .min_width(width)
            .max_width(width)
            .max_height(POPUP_MAX_HEIGHT),
    )
    .into()
}

/// Format a Unix timestamp as a human-readable date/time string.
pub fn format_timestamp(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    let datetime = Local.timestamp_millis_opt(timestamp).single();
    match datetime {
        Some(dt) => {
            let now = Local::now();
            if dt.date_naive() == now.date_naive() {
                dt.format("%H:%M").to_string()
            } else {
                dt.format("%b %d").to_string()
            }
        }
        None => "Unknown".to_string(),
    }
}

/// Format milliseconds as mm:ss time string.
pub fn format_duration(ms: i64) -> String {
    if ms <= 0 {
        return "0:00".to_string();
    }
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{}:{:02}", minutes, seconds)
}
