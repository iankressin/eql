use crate::interpreter::{
    backend::execution_engine::{ExpressionResult, QueryResult},
    Interpreter, InterpreterResultHandler,
};
use crossterm::{
    cursor::{MoveLeft, MoveRight, MoveTo, MoveToColumn, MoveToNextLine},
    event::{read, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Print, Stylize},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::io::{stdout, Stdout, Write};
use tabled::{settings::Style, Table};

static REPL_LABEL: &str = "EQL >";

pub struct Repl {
    history: Vec<String>,
    stdout: Stdout,
}

impl Repl {
    pub fn new() -> Self {
        Repl {
            history: vec![],
            stdout: stdout(),
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut history_offset: usize = 0;
        let mut stdout = stdout();
        let mut expression = String::new();
        let mut cursor_pos = 1;

        self.clear_screen()?;
        enable_raw_mode()?;

        loop {
            let event = read()?;

            match event {
                Event::Key(key_event) => match key_event.code {
                    KeyCode::Char(ch) => {
                        if key_event.modifiers == KeyModifiers::CONTROL && ch == 'c' {
                            break;
                        }
                        else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'l' {
                            cursor_pos = 1;
                            self.clear_screen()?;
                            continue;
                        }
                        else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'u' {
                            cursor_pos = 1;
                            expression.clear();
                            self.redraw_line(&expression, cursor_pos)?;
                        }
                        else {
                            expression.insert(cursor_pos - 1, ch);
                            cursor_pos += 1;
                            self.redraw_line(&expression, cursor_pos)?;
                        }
                    }
                    KeyCode::Backspace => {
                        if key_event.modifiers == KeyModifiers::ALT {
                            let mut temp = expression.split(" ").collect::<Vec<&str>>();

                            if let Some(removed) = temp.pop() {
                                if temp.len() == 0 {
                                    continue;
                                } else {
                                    let removed_len = removed.len() + 1; // +1 for the space
                                    cursor_pos -= removed_len;
                                    expression = temp.join(" ");
                                }
                            }

                        } else {
                            expression.pop();

                            if cursor_pos > 1 {
                                cursor_pos -= 1;
                            }
                        }

                        self.redraw_line(&expression, cursor_pos)?;
                    }
                    KeyCode::Enter => {
                        self.history.push(expression.trim().to_string());

                        let _ = Repl::run_expression(&expression.trim()).await.map_err(|e| {
                            for line in e.to_string().split("\n") {
                                queue!(stdout, MoveToNextLine(1), Print(line.red())).unwrap();
                            }
                        });

                        expression.clear();
                        cursor_pos = 1;
                        history_offset = 0;

                        queue!(
                            stdout,
                            MoveToNextLine(1),
                            Print(REPL_LABEL.italic().dark_grey().on_dark_yellow()),
                            Print(" "),
                            Print(&expression),
                        )?;
                    }
                    KeyCode::Up => {
                        if self.history.len() == 0 || history_offset >= self.history.len() {
                            continue;
                        }

                        history_offset += 1;
                        expression = self.history[self.history.len() - history_offset].clone();
                        cursor_pos = expression.len() + 1;
                        self.redraw_line(&expression, cursor_pos)?;
                    }
                    KeyCode::Down => {
                        if history_offset == 0 {
                            continue;
                        }

                        history_offset -= 1;
                        expression = if history_offset == 0 {
                            cursor_pos = 1;
                            "".to_string()
                        } else {
                            let temp = self.history[self.history.len() - history_offset].clone();
                            cursor_pos = temp.len() + 1;
                            temp
                        };

                        self.redraw_line(&expression, cursor_pos)?;
                    }
                    KeyCode::Left => {
                        if cursor_pos > 0 {
                            cursor_pos -= 1;
                        }
                        queue!(stdout, MoveLeft(1))?;
                    }
                    KeyCode::Right => {
                        if cursor_pos < expression.len() {
                            cursor_pos += 1;
                        }
                        queue!(stdout, MoveRight(1))?;
                    }
                    KeyCode::Esc => break,
                    _ => {}
                },
                _ => {}
            }

            stdout.flush()?;
        }

        disable_raw_mode()?;

        Ok(())
    }

    fn redraw_line(
        &mut self,
        expression: &str,
        cursor_pos: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

    async fn run_expression(expression: &str) -> Result<(), Box<dyn std::error::Error>> {
        Interpreter::new(ResultHandler::new())
            .run_program(expression)
            .await?;

        Ok(())
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

struct ResultHandler;

impl ResultHandler {
    pub fn new() -> Self {
        ResultHandler
    }
}

impl InterpreterResultHandler for ResultHandler {
    // TODO: this can be refactored to be more generic
    fn handle_result(&self, query_results: Vec<QueryResult>) {
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
}
