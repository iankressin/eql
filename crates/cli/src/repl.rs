use crossterm::{
    cursor::{MoveLeft, MoveRight, MoveTo, MoveToColumn, MoveToNextLine},
    event::{read, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Print, Stylize},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use eql_core::interpreter::{
    backend::execution_engine::{ExpressionResult, QueryResult},
    Interpreter,
};
use std::io::{stdout, Stdout, Write};
use tabled::{settings::Style, Table};

static REPL_LABEL: &str = "EQL >";

pub struct Repl {
    history: Vec<String>,
    history_offset: usize,
    stdout: Stdout,
    cursor_pos: usize,
    expression: String,
}

impl Repl {
    pub fn new() -> Self {
        Repl {
            history: vec![],
            stdout: stdout(),
            cursor_pos: 1,
            expression: String::new(),
            history_offset: 0,
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.clear_screen()?;
        enable_raw_mode()?;

        loop {
            let event = read()?;

            match event {
                Event::Key(key_event) => match key_event.code {
                    KeyCode::Char(ch) => {
                        if key_event.modifiers == KeyModifiers::CONTROL && ch == 'c' {
                            break;
                        } else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'l' {
                            self.cursor_pos = 1;
                            self.clear_screen()?;
                            continue;
                        } else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'u' {
                            self.cursor_pos = 1;
                            self.expression.clear();
                            self.redraw_line()?;
                        } else {
                            self.expression.insert(self.cursor_pos - 1, ch);
                            self.cursor_pos += 1;
                            self.redraw_line()?;
                        }
                    }
                    KeyCode::Backspace => {
                        if key_event.modifiers == KeyModifiers::ALT {
                            let mut temp = self.expression.split(" ").collect::<Vec<&str>>();

                            if let Some(removed) = temp.pop() {
                                if temp.len() == 0 {
                                    continue;
                                } else {
                                    let removed_len = removed.len() + 1; // +1 for the space
                                    self.cursor_pos -= removed_len;
                                    self.expression = temp.join(" ");
                                }
                            }
                        } else {
                            self.expression.pop();

                            if self.cursor_pos > 1 {
                                self.cursor_pos -= 1;
                            }
                        }

                        self.redraw_line()?;
                    }
                    KeyCode::Enter => {
                        let _ = self.run_expression().await.map_err(|e| {
                            for line in e.to_string().split("\n") {
                                queue!(self.stdout, MoveToNextLine(1), Print(line.red()),).unwrap();
                            }
                        });

                        self.history.push(self.expression.trim().to_string());
                        self.expression.clear();
                        self.cursor_pos = 1;
                        self.history_offset = 0;

                        queue!(
                            self.stdout,
                            MoveToNextLine(1),
                            Print(REPL_LABEL.italic().dark_grey().on_dark_yellow()),
                            Print(" "),
                            Print(&self.expression),
                        )?;
                    }
                    KeyCode::Up => {
                        if self.history.len() == 0 || self.history_offset >= self.history.len() {
                            continue;
                        }

                        self.history_offset += 1;
                        self.expression =
                            self.history[self.history.len() - self.history_offset].clone();
                        self.cursor_pos = self.expression.len() + 1;
                        self.redraw_line()?;
                    }
                    KeyCode::Down => {
                        if self.history_offset == 0 {
                            continue;
                        }

                        self.history_offset -= 1;
                        self.expression = if self.history_offset == 0 {
                            self.cursor_pos = 1;
                            "".to_string()
                        } else {
                            let temp =
                                self.history[self.history.len() - self.history_offset].clone();
                            self.cursor_pos = temp.len() + 1;
                            temp
                        };

                        self.redraw_line()?;
                    }
                    KeyCode::Left => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                        }
                        queue!(self.stdout, MoveLeft(1))?;
                    }
                    KeyCode::Right => {
                        if self.cursor_pos < self.expression.len() {
                            self.cursor_pos += 1;
                        }
                        queue!(self.stdout, MoveRight(1))?;
                    }
                    KeyCode::Esc => break,
                    _ => {}
                },
                _ => {}
            }

            self.stdout.flush()?;
        }

        disable_raw_mode()?;

        Ok(())
    }

    fn redraw_line(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let expression = self.expression.clone();
        let cursor_pos = self.cursor_pos;

        queue!(
            self.stdout,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            Print(REPL_LABEL.italic().dark_grey().on_dark_yellow()),
            Print(" "),
            Print(expression),
            MoveToColumn(cursor_pos as u16 + 5),
        )?;

        Ok(())
    }

    async fn run_expression(&self) -> Result<(), Box<dyn std::error::Error>> {
        let result = Interpreter::run_program(&self.expression).await?;
        self.display_result(result);
        Ok(())
    }

    fn display_result(&self, query_results: Vec<QueryResult>) {
        for query_result in query_results {
            match query_result.result {
                ExpressionResult::Account(query_res) => {
                    let mut table = Table::new(vec![query_res]);
                    table.with(Style::rounded());
                    table.to_string().split("\n").for_each(|line| {
                        queue!(stdout(), MoveToNextLine(1), Print(line.magenta())).unwrap();
                    });
                }
                ExpressionResult::Block(query_res) => {
                    let mut table = Table::new(vec![query_res]);
                    table.with(Style::rounded());
                    table.to_string().split("\n").for_each(|line| {
                        queue!(stdout(), MoveToNextLine(1), Print(line.cyan())).unwrap();
                    });
                }
                ExpressionResult::Transaction(query_res) => {
                    let mut table = Table::new(vec![query_res]);
                    table.with(Style::rounded());
                    table.to_string().split("\n").for_each(|line| {
                        queue!(stdout(), MoveToNextLine(1), Print(line.yellow())).unwrap();
                    });
                }
            }
        }
    }

    fn clear_screen(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        execute!(
            self.stdout,
            Clear(ClearType::All),
            MoveTo(0, 0),
            Print("EQL REPL - Press ESC to exit"),
            MoveToNextLine(1),
            Print("EQL >".italic().dark_grey().on_dark_yellow()),
            Print(" "),
        )?;

        Ok(())
    }
}
