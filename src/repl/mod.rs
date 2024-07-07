use crate::interpreter::{
    backend::execution_engine::{ExpressionResult, QueryResult},
    Interpreter, InterpreterResultHandler,
};
use crossterm::{
    cursor::{MoveLeft, MoveTo, MoveToColumn, MoveToNextLine},
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
                            self.clear_screen()?;
                            continue;
                        }
                        else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'u' {
                            expression.clear();
                            self.redraw_line(&expression)?;
                        }
                        else {
                            expression.push(ch);
                            self.redraw_line(&expression)?;
                        }
                    }
                    KeyCode::Backspace => {
                        expression.pop();
                        self.redraw_line(&expression)?;
                    }
                    KeyCode::Enter => {
                        self.history.push(expression.clone());

                        let _ = Repl::run_expression(&expression.trim()).await.map_err(|e| {
                            for line in e.to_string().split("\n") {
                                queue!(stdout, MoveToNextLine(1), Print(line.red())).unwrap();
                            }
                        });

                        expression.clear();
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
                        execute!(
                            stdout,
                            MoveToColumn(0),
                            Clear(ClearType::CurrentLine),
                            Print(history_offset.to_string())
                        )?;

                        if history_offset >= self.history.len() {
                            history_offset = self.history.len() - 1;
                        }

                        expression = self.history[history_offset].clone();
                        self.redraw_line(&expression)?;

                        history_offset += 1;
                    }
                    KeyCode::Down => {
                        if history_offset == 0 {
                            continue;
                        } else {
                            history_offset -= 1;
                            expression = self.history[history_offset].clone();
                        }

                        self.redraw_line(&expression)?;
                    }
                    KeyCode::Left => {
                        queue!(stdout, MoveLeft(1))?;
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        queue!(
            self.stdout,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            Print(REPL_LABEL.italic().dark_grey().on_dark_yellow()),
            Print(" "),
            Print(expression),
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
