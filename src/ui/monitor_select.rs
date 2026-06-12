use gpui::{
    AppContext, InteractiveElement, ParentElement, Render, Styled, div, px, size
};

use crate::ui::consts::{BG_COLOR, TEXT_COLOR};
pub struct MonitorSelect {
    pub display: gpui::DisplayId,
    pub sender: smol::channel::Sender<gpui::DisplayId>,
}
impl Render for MonitorSelect {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let display = self.display;
        let sender = self.sender.clone();
        div()
            .id("btn")
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, _| {
                smol::block_on(sender.send(display)).expect("Failed to send display");
            })
            .child("Select me")
            .bg(BG_COLOR)
            .text_color(TEXT_COLOR)
            .text_align(gpui::TextAlign::Center)
            .self_center()
            .content_stretch()
    }
}
pub fn new_monitor_select(
    cx: &mut gpui::App,
    display: gpui::DisplayId,
    callback: smol::channel::Sender<gpui::DisplayId>,
) -> gpui::WindowHandle<MonitorSelect> {
    println!("Opening window for {:?}", display);
    cx.open_window(
        gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                gpui::Point {
                    ..Default::default()
                },
                size(px(500.), px(500.)),
            ))),
            window_background: gpui::WindowBackgroundAppearance::Opaque,
            kind: gpui::WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
                // anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                layer: gpui::layer_shell::Layer::Overlay,
                exclusive_zone: None,
                keyboard_interactivity: gpui::layer_shell::KeyboardInteractivity::None,
                ..Default::default()
            }),
            display_id: Some(display),
            focus: false,
            ..Default::default()
        },
        |_, cx| {
            cx.new(|_| MonitorSelect {
                display,
                sender: callback,
            })
        },
    )
    .expect("Window failed to open")
}
