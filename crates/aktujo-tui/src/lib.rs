// SPDX-License-Identifier: AGPL-3.0-or-later
//! The terminal interface of aktujo: everything with the keyboard, and with the mouse – a click does what the key or
//! the row under it does, the wheel moves like the arrow keys; `m` hands the mouse back to the terminal (to select
//! text). It works through the workspace of `aktujo-app` only, like every other interface.

use ratatui::{
    Frame,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, MouseButton,
            MouseEvent, MouseEventKind,
        },
        execute,
    },
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};

use aktujo_app::Workspace;

/// Why the interface ended with an error.
#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    /// The terminal.
    #[error("terminal: {0}")]
    Terminal(#[from] std::io::Error),
}

/// The actions at the bottom, each with its key – clickable as well.
const ACTIONS: [(char, &str); 4] = [('a', "add"), ('d', "delete"), ('m', "mouse"), ('q', "quit")];

/// The state of the interface; drawing it is [`draw`], changing it [`App::key`] and [`App::mouse`].
#[derive(Debug)]
pub struct App<'a> {
    workspace: &'a mut Workspace,
    selected: Option<usize>,
    /// The title being typed, while adding.
    input: Option<String>,
    message: String,
    /// Whether the interface has the mouse.
    pub mouse: bool,
    /// Whether to end.
    pub quit: bool,
    /// Where the last frame put the list and the actions, for clicks.
    list: Rect,
    actions: Vec<(Rect, char)>,
}

impl<'a> App<'a> {
    /// The interface on a workspace.
    pub fn new(workspace: &'a mut Workspace) -> Self {
        let selected = (!workspace.items().is_empty()).then_some(0);
        App {
            workspace,
            selected,
            input: None,
            message: String::new(),
            mouse: true,
            quit: false,
            list: Rect::default(),
            actions: Vec::new(),
        }
    }

    /// Handles a key.
    pub fn key(&mut self, key: KeyEvent) {
        if let Some(input) = &mut self.input {
            match key.code {
                KeyCode::Enter => {
                    let text = std::mem::take(input);
                    self.input = None;
                    match self.workspace.add_item(&text) {
                        Ok(item) => {
                            self.message = format!("added {}", item.title);
                            self.selected = Some(self.workspace.items().len() - 1);
                        }
                        Err(error) => self.message = error.to_string(),
                    }
                }
                KeyCode::Esc => self.input = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            }
            return;
        }
        let count = self.workspace.items().len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('a') => self.input = Some(String::new()),
            KeyCode::Char('m') => self.mouse = !self.mouse,
            KeyCode::Char('d') | KeyCode::Delete => self.delete(),
            KeyCode::Down if count > 0 => self.selected = Some(self.selected.map_or(0, |s| (s + 1).min(count - 1))),
            KeyCode::Up if count > 0 => self.selected = Some(self.selected.map_or(0, |s| s.saturating_sub(1))),
            _ => {}
        }
    }

    /// Handles the mouse.
    pub fn mouse(&mut self, event: MouseEvent) {
        let at = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown => self.key(KeyCode::Down.into()),
            MouseEventKind::ScrollUp => self.key(KeyCode::Up.into()),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(&(_, key)) = self.actions.iter().find(|(area, _)| area.contains(at)) {
                    self.key(KeyCode::Char(key).into());
                } else if self.input.is_none() && self.list.contains(at) {
                    let row = usize::from(at.y - self.list.y).checked_sub(1);
                    if let Some(row) = row.filter(|row| *row < self.workspace.items().len()) {
                        self.selected = Some(row);
                    }
                }
            }
            _ => {}
        }
    }

    fn delete(&mut self) {
        let Some(at) = self.selected else { return };
        let id = self.workspace.items()[at].id;
        match self.workspace.remove_item(id) {
            Ok(item) => self.message = format!("deleted {}", item.title),
            Err(error) => self.message = error.to_string(),
        }
        let count = self.workspace.items().len();
        self.selected = (count > 0).then(|| at.min(count - 1));
    }
}

/// Draws the interface and remembers where the clickable parts are.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let [list, input, bar] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(3), Constraint::Length(1)]).areas(frame.area());
    let rows: Vec<ListItem> = app.workspace.items().iter().map(|item| ListItem::new(item.title.as_str())).collect();
    let mut state = ListState::default().with_selected(app.selected);
    let highlight = Style::default().add_modifier(Modifier::REVERSED);
    frame.render_stateful_widget(
        List::new(rows).block(Block::bordered().title(" aktujo ")).highlight_style(highlight),
        list,
        &mut state,
    );
    app.list = list;
    let text = match &app.input {
        Some(input) => format!("title: {input}▏ (Enter adds, Esc cancels)"),
        None => app.message.clone(),
    };
    frame.render_widget(Paragraph::new(text).block(Block::bordered()), input);
    app.actions.clear();
    let mut spans = Vec::new();
    let mut x = bar.x;
    for (key, label) in ACTIONS {
        let label = if key == 'm' && !app.mouse { "mouse off" } else { label };
        let text = format!(" {key} {label} ");
        let width = text.chars().count() as u16;
        app.actions.push((Rect::new(x, bar.y, width, 1), key));
        spans.push(Span::styled(text, highlight));
        spans.push(Span::raw(" "));
        x += width + 1;
    }
    frame.render_widget(Line::from(spans), bar);
}

/// Runs the terminal interface on a workspace until it is left.
pub fn run(workspace: &mut Workspace) -> Result<(), TuiError> {
    let mut app = App::new(workspace);
    let mut terminal = ratatui::try_init()?;
    let mut mouse = false;
    let result = (|| -> Result<(), TuiError> {
        while !app.quit {
            if app.mouse != mouse {
                mouse = app.mouse;
                if mouse {
                    execute!(std::io::stdout(), EnableMouseCapture)?;
                } else {
                    execute!(std::io::stdout(), DisableMouseCapture)?;
                }
            }
            terminal.draw(|frame| draw(frame, &mut app))?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.key(key),
                Event::Mouse(event) if app.mouse => app.mouse(event),
                _ => {}
            }
        }
        Ok(())
    })();
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, crossterm::event::KeyModifiers};

    use super::*;

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, modifiers: KeyModifiers::NONE }
    }

    #[test]
    fn keys_and_clicks_do_the_same() {
        let mut workspace = Workspace::new();
        let mut app = App::new(&mut workspace);
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        // Click "a add", type, Enter.
        app.mouse(click(1, 11));
        for c in "tea".chars() {
            app.key(KeyCode::Char(c).into());
        }
        app.key(KeyCode::Enter.into());
        app.key(KeyCode::Char('a').into());
        for c in "milk".chars() {
            app.key(KeyCode::Char(c).into());
        }
        app.key(KeyCode::Enter.into());
        assert_eq!(app.selected, Some(1));
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        app.mouse(click(3, 1));
        assert_eq!(app.selected, Some(0));
        app.key(KeyCode::Char('d').into());
        assert_eq!(app.workspace.items().len(), 1);
        assert_eq!(app.workspace.items()[0].title, "milk");
        app.mouse(click(39, 11));
        assert!(!app.quit, "nothing there");
        app.mouse(click(30, 11));
        assert!(app.quit, "q quit");
    }
}
