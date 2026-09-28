use crossterm::event::{Event as CrosstermEvent, KeyEvent, MouseButton, MouseEventKind};

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Key(KeyEvent),
    Mouse {
        kind: MouseEventKind,
        column: u16,
        row: u16,
    },
    Resize(u16, u16),
}

impl From<CrosstermEvent> for Event {
    fn from(event: CrosstermEvent) -> Self {
        match event {
            CrosstermEvent::Key(key) => Self::Key(key),
            CrosstermEvent::Mouse(mouse) => Self::Mouse {
                kind: mouse.kind,
                column: mouse.column,
                row: mouse.row,
            },
            CrosstermEvent::Resize(width, height) => Self::Resize(width, height),
            _ => Self::Resize(0, 0),
        }
    }
}

pub fn is_left_click(kind: MouseEventKind) -> bool {
    matches!(kind, MouseEventKind::Down(MouseButton::Left))
}
