use std::{time::Duration};

use gpui::{
    AppContext, Element, Entity, ParentElement, Render, SharedString, Styled, WeakEntity,
    WindowHandle, div, layer_shell::Anchor, px, rgb, size,
};

use crate::ui::{
    clock::Clock,
    consts::{self, BAR_HEIGHT, BG_COLOR, FONT_NAME, FONT_WEIGHT, TEXT_COLOR, TEXT_SIZE},
    workspaces::Workspaces,
};

pub struct Bar {
    pub window_title_text: SharedString,
    pub clock: Entity<Clock>,
    pub workspaces: Entity<Workspaces>,
}
impl Render for Bar {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        div()
            .flex()
            .flex_row()
            .flex_auto()
            .size_full()
            .justify_between()
            .content_around()
            .text_color(TEXT_COLOR)
            .font(gpui::Font {
                family: FONT_NAME.into(),
                weight: gpui::FontWeight(FONT_WEIGHT),
                ..Default::default()
            })
            .text_size(TEXT_SIZE)
            .text_align(gpui::TextAlign::Left)
            .self_center()
            .bg(BG_COLOR)
            .child(self.workspaces.clone())
            .child(self.window_title_text.clone())
            .child(self.clock.clone())
    }
}

pub fn open_window<T: Render>(app: &mut gpui::AsyncApp, root: Entity<T>, display: gpui::DisplayId) -> WindowHandle<T> {
    app.open_window(
        gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                gpui::Point {
                    ..Default::default()
                },
                size(px(0.), BAR_HEIGHT),
            ))),
            window_background: gpui::WindowBackgroundAppearance::Blurred,
            kind: gpui::WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
                anchor: Anchor::LEFT | Anchor::RIGHT | Anchor::TOP,
                layer: gpui::layer_shell::Layer::Top,
                exclusive_zone: Some(BAR_HEIGHT),
                keyboard_interactivity: gpui::layer_shell::KeyboardInteractivity::None,
                ..Default::default()
            }),
            display_id: Some(display),
            focus: false,
            ..Default::default()
        },
        |_, _| root,
    )
    .expect("Failed to open window")
}
pub fn init_bar(cx: &mut gpui::AsyncApp, workspaces: Entity<Workspaces>) -> Entity<Bar> {
    cx.new(|cx| Bar {
        window_title_text: "".into(),
        clock: Clock::new_entity(cx),
        workspaces,
    })
}
