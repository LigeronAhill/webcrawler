use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Offset, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Widget;
use ratatui::{DefaultTerminal, Frame};

#[derive(Default)]
pub struct App {
    state: AppState,
    form: InputForm,
}

#[derive(Default, PartialEq, Eq)]
enum AppState {
    #[default]
    Running,
    Cancelled,
    Submitted,
}

impl App {
    // pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<Option<InputForm>> {
    //     while self.state == AppState::Running {
    //         terminal.draw(|frame| self.render(frame))?;
    //         self.handle_events()?;
    //     }
    //     match self.state {
    //         AppState::Cancelled => Ok(None),
    //         AppState::Submitted => Ok(Some(self.form)),
    //         AppState::Running => unreachable!(),
    //     }
    // }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<Option<InputForm>> {
        while self.state == AppState::Running {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_events()?;
        }

        // Показываем сообщение о завершении
        if self.state == AppState::Submitted {
            terminal.draw(|frame| {
                let area = frame.area();
                let text = Line::from("Starting crawler... Press any key to exit");
                text.render(area, &mut Buffer::empty(area));
            })?;
            std::thread::sleep(std::time::Duration::from_secs(2));
        }

        match self.state {
            AppState::Cancelled => Ok(None),
            AppState::Submitted => Ok(Some(self.form)),
            AppState::Running => unreachable!(),
        }
    }
    fn render(&self, frame: &mut Frame) {
        self.form.render(frame);
    }

    fn handle_events(&mut self) -> Result<()> {
        if let Some(key) = event::read()?.as_key_press_event() {
            match key.code {
                KeyCode::Esc => self.state = AppState::Cancelled,
                KeyCode::Enter => self.state = AppState::Submitted,
                _ => self.form.on_key_press(key),
            }
        }
        Ok(())
    }
}

pub struct InputForm {
    pub focus: Focus,
    pub start_url: StringField,
    pub max_depth: DepthField,
}

impl Default for InputForm {
    fn default() -> Self {
        Self {
            focus: Focus::StartUrl,
            start_url: StringField::new("Start URL"),
            max_depth: DepthField::new("Max depth"),
        }
    }
}

impl InputForm {
    // Handle focus navigation or pass the event to the focused field.
    fn on_key_press(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Tab => self.focus = self.focus.next(),
            _ => match self.focus {
                Focus::StartUrl => self.start_url.on_key_press(event),
                Focus::MaxDepth => self.max_depth.on_key_press(event),
            },
        }
    }

    /// Render the form with the current focus.
    ///
    /// The cursor is placed at the end of the focused field.
    fn render(&self, frame: &mut Frame) {
        let layout = Layout::vertical(Constraint::from_lengths([1, 1]));
        let [start_url_area, max_depth_area] = frame.area().layout(&layout);

        frame.render_widget(&self.start_url, start_url_area);
        frame.render_widget(&self.max_depth, max_depth_area);

        let cursor_position = match self.focus {
            Focus::StartUrl => start_url_area + self.start_url.cursor_offset(),
            Focus::MaxDepth => max_depth_area + self.max_depth.cursor_offset(),
        };
        frame.set_cursor_position(cursor_position);
    }
}

#[derive(Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    StartUrl,
    MaxDepth,
}

impl Focus {
    // Round-robin focus order.
    const fn next(&self) -> Self {
        match self {
            Self::StartUrl => Self::MaxDepth,
            Self::MaxDepth => Self::StartUrl,
        }
    }
}

/// A new-type representing a string field with a label.
pub struct StringField {
    pub label: &'static str,
    pub value: String,
}

impl StringField {
    const fn new(label: &'static str) -> Self {
        Self {
            label,
            value: String::new(),
        }
    }

    /// Handle input events for the string input.
    fn on_key_press(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Char(c) => self.value.push(c),
            KeyCode::Backspace => {
                self.value.pop();
            }
            _ => {}
        }
    }

    const fn cursor_offset(&self) -> Offset {
        let x = (self.label.len() + self.value.len() + 2) as i32;
        Offset::new(x, 0)
    }
}

impl Widget for &StringField {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::horizontal([
            Constraint::Length(self.label.len() as u16 + 2),
            Constraint::Fill(1),
        ]);
        let [label_area, value_area] = area.layout(&layout);
        let label = Line::from_iter([self.label, ": "]).bold();
        label.render(label_area, buf);
        self.value.clone().render(value_area, buf);
    }
}

/// A new-type representing a person's age in years (0-130).
#[derive(Default, Clone, Copy)]
pub struct DepthField {
    pub label: &'static str,
    pub value: u8,
}

impl DepthField {
    const MAX: u8 = 130;

    const fn new(label: &'static str) -> Self {
        Self { label, value: 0 }
    }

    /// Handle input events for the age input.
    ///
    /// Digits are accepted as input, with any input which would exceed the maximum age being
    /// ignored. The up/down arrow keys and 'j'/'k' keys can be used to increment/decrement the
    /// age.
    fn on_key_press(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Char(digit @ '0'..='9') => {
                let value = self
                    .value
                    .saturating_mul(10)
                    .saturating_add(digit as u8 - b'0');
                if value <= Self::MAX {
                    self.value = value;
                }
            }
            KeyCode::Backspace => self.value /= 10,
            KeyCode::Up | KeyCode::Char('k') => self.increment(),
            KeyCode::Down | KeyCode::Char('j') => self.decrement(),
            _ => {}
        }
    }

    fn increment(&mut self) {
        self.value = self.value.saturating_add(1).min(Self::MAX);
    }

    const fn decrement(&mut self) {
        self.value = self.value.saturating_sub(1);
    }

    fn cursor_offset(&self) -> Offset {
        let x = (self.label.len() + self.value.to_string().len() + 2) as i32;
        Offset::new(x, 0)
    }
}

impl Widget for &DepthField {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::horizontal([
            Constraint::Length(self.label.len() as u16 + 2),
            Constraint::Fill(1),
        ]);
        let [label_area, value_area] = area.layout(&layout);
        let label = Line::from_iter([self.label, ": "]).bold();
        let value = self.value.to_string();
        label.render(label_area, buf);
        value.render(value_area, buf);
    }
}
