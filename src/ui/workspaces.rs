use std::sync::Arc;

use gpui::{AppContext, AsyncApp, Entity, ParentElement, Render, Styled, div, rgb};

use crate::ui::{consts::TEXT_COLOR, event::Workspace};
pub struct Workspaces {
    pub workspaces: Vec<Arc<Workspace>>,
    pub focused_workspace: u64,
    pub output: String,
}
impl Render for Workspaces {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let mut root = div();
        self.workspaces.sort_by(|a, b| a.idx.cmp(&b.idx));
        for workspace in self.workspaces.iter() {
            if self.output != workspace.output {continue};
            root = root.child(
                div()
                    .text_color(if workspace.id == self.focused_workspace {
                        rgb(0xff0000)
                    } else {
                        TEXT_COLOR
                    })
                    .child(workspace.idx.to_string()),
            );
        }
        root.flex().flex_row().flex_grow_0()
    }
}

impl Workspaces {
    pub fn new_entity(cx: &mut gpui::AsyncApp, output: String) -> Entity<Workspaces> {
        cx.new(|_| Workspaces {
            workspaces: vec![Arc::new(Workspace {
                ..Default::default()
            })],
            focused_workspace: 0,
            output,
        })
    }
}
