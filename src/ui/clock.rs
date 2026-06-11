use std::time::Duration;

use gpui::{App, AppContext, Entity, ParentElement, Render, Styled, div, px};
pub struct Clock {}
impl Render for Clock {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let now = chrono::Local::now();
        let time = now.format("%H:%M").to_string();
        div()
            .text_align(gpui::TextAlign::Right)
            .flex_basis(px(0.))
            .child(time)
    }
}
impl Clock {
    pub fn new_entity(cx: &mut App) -> Entity<Clock> {
        cx.new(|cx| {
            let clock = Clock {};
            cx.spawn(async |entity, app| {
                loop {
                    entity
                        .update(app, |_clock: &mut Clock, cx| cx.notify())
                        .ok();
                    app.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                }
            })
            .detach();
            clock
        })
    }
}
