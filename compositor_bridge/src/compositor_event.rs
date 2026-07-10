use core::fmt;
use std::sync::Arc;

use crate::state::*;
#[derive(Clone)]
pub enum CompositorEvent {
    FocusedWindowChanged(Arc<Window>),
    WorkspacesChanged(Vec<Arc<Workspace>>),
    WorkspaceFocusChanged(usize),
}
impl fmt::Debug for CompositorEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkspacesChanged(_) => f
                .debug_tuple("EventWithIterator")
                .field(&"<iterator>")
                .finish(),
            _ => write!(f, "StandardEvent"),
        }
    }
}
