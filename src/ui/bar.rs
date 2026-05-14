use gpui::{
    AppContext, Entity, ParentElement, Render, SharedString, Styled, WindowHandle, div, layer_shell::Anchor, px, rgb, size
};

const BG_COLOR: gpui::Rgba = gpui::Rgba {
    a: 1.,
    g: 0.,
    r: 0.,
    b: 0.,
};
const BAR_HEIGHT: f32 = 20.;

#[derive(Debug)]
pub struct Bar {
    pub window_title_text: SharedString,
}
impl Render for Bar {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        div()
            .size_full()
            .text_color(rgb(0xffffff))
            .bg(BG_COLOR)
            .child(self.window_title_text.clone())
    }
}

pub fn open_window<T: Render>(app:&mut gpui::App, root: Entity<T>) -> WindowHandle<T> {
        app.open_window(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::new(
                    gpui::Point {
                        ..Default::default()
                    },
                    size(px(500.), px(BAR_HEIGHT)),
                ))),
                window_background: gpui::WindowBackgroundAppearance::Blurred,

                kind: gpui::WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
                    anchor: Anchor::LEFT | Anchor::RIGHT | Anchor::TOP,
                    layer: gpui::layer_shell::Layer::Top,
                    exclusive_zone: Some(px(BAR_HEIGHT)),
                    keyboard_interactivity: gpui::layer_shell::KeyboardInteractivity::None,
                    ..Default::default()
                }),
                focus: false,
                ..Default::default()
            },
            |_, _| root,
        ).expect("Failed to open window")
}
pub fn init_bar(cx: &mut gpui::App) -> Entity<Bar> {
    cx.new(|_| Bar {
        window_title_text: "".into(),
    })
}
