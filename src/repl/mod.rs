use crate::interpreter::{
    backend::execution_engine::{ExpressionResult, QueryResult},
    Interpreter, InterpreterResultHandler,
};
use crossterm::{
    cursor::{MoveLeft, MoveTo, MoveToColumn, MoveToNextLine},
    event::{read, Event, KeyCode},
    execute, queue,
    style::{Print, Stylize},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::io::{stdout, Stdout, Write};
use tabled::{settings::Style, Table};

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
        // TODO: make label static
        let label = String::from("EQL >");
        let mut history_offset: usize = 0;
        let mut stdout = stdout();
        // let mut expression = String::new();
        let mut expression = String::from(
            "GET nonce, balance FROM account 0x00000000219ab540356cBB839Cbe05303d7705Fa ON eth",
        );
        execute!(
            stdout,
            Clear(ClearType::All),
            MoveTo(0, 0),
            Print(label.clone().italic().dark_grey().on_dark_yellow()),
            Print(" "),
        )?;

        enable_raw_mode()?;

        loop {
            let event = read()?;

            match event {
                Event::Key(key_event) => match key_event.code {
                    KeyCode::Char(ch) => {
                        // TODO:
                        // - ctrl + c => break loop
                        // - ctrl + l => clear screen
                        // - ctrl + u => clear line
                        // - ctrl + a => move to start of line
                        // - ctrl + e => move to end of line
                        expression.push(ch);
                        self.redraw_line(&label, &expression)?;
                    }
                    KeyCode::Backspace => {
                        expression.pop();
                        self.redraw_line(&label, &expression)?;
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
                            Print(label.clone().italic().dark_grey().on_dark_yellow()),
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
                        self.redraw_line(&label, &expression)?;

                        history_offset += 1;
                    }
                    KeyCode::Down => {
                        if history_offset == 0 {
                            continue;
                        } else {
                            history_offset -= 1;
                            expression = self.history[history_offset].clone();
                        }

                        self.redraw_line(&label, &expression)?;
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
        label: &str,
        expression: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // let line = format!("{}{}", label, expression);
        queue!(
            self.stdout,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            Print(label.italic().dark_grey().on_dark_yellow()),
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
