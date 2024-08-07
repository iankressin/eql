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

    // The main loop for the REPL session
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Clear the screen and enable raw mode for the terminal
        self.clear_screen()?;
        enable_raw_mode()?;

        loop {
            // Read an event from the terminal
            let event = read()?;

            match event {
                Event::Key(key_event) => match key_event.code {
                    // Handle character input
                    KeyCode::Char(ch) => {
                        // Exit on Ctrl+C
                        if key_event.modifiers == KeyModifiers::CONTROL && ch == 'c' {
                            break;
                        // Clear screen on Ctrl+L
                        } else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'l' {
                            self.cursor_pos = 1;
                            self.clear_screen()?;
                            continue;
                        // Clear current expression on Ctrl+U
                        } else if key_event.modifiers == KeyModifiers::CONTROL && ch == 'u' {
                            self.cursor_pos = 1;
                            self.expression.clear();
                            self.redraw_line()?;
                        // Insert character into the current expression
                        } else {
                            self.expression.insert(self.cursor_pos - 1, ch);
                            self.cursor_pos += 1;
                            self.redraw_line()?;
                        }
                    }
                    // Handle backspace
                    KeyCode::Backspace => {
                        // Delete word on Alt+Backspace
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
                        // Delete character on Backspace
                        } else {
                            if self.cursor_pos > 1 {
                                self.expression.remove(self.cursor_pos -2); // -2 because it's the previous index and cursor start at 1 while string indexes at 0.
                            }

                            if self.cursor_pos > 1 {
                                self.cursor_pos -= 1;
                            }
                        }

                        self.redraw_line()?;
                    }
                    
                    // Delete character on Delete
                    KeyCode::Delete => {
                        if self.cursor_pos -1 < self.expression.len() { 
                            self.expression.remove(self.cursor_pos -1); // -1 because cursor start at 1 while string indexes at 0.
                            self.redraw_line()?;
                        }
                    }
                    // Handle Enter key to execute the current expression
                    KeyCode::Enter => {
                        // Run the current expression and handle any errors
                        let _ = self.run_expression().await.map_err(|e| {
                            for line in e.to_string().split("\n") {
                                queue!(self.stdout, MoveToNextLine(1), Print(line.red()),).unwrap();
                            }
                        });

                        // Add the current expression to history and reset the state
                        self.history.push(self.expression.trim().to_string());
                        self.expression.clear();
                        self.cursor_pos = 1;
                        self.history_offset = 0;

                        // Display the REPL prompt
                        queue!(
                            self.stdout,
                            MoveToNextLine(1),
                            Print(REPL_LABEL.italic().dark_grey().on_dark_yellow()),
                            Print(" "),
                            Print(&self.expression),
                        )?;
                    }
                    // Handle Up arrow key to navigate history
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
                    // Handle Down arrow key to navigate history
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
                    // Handle Left arrow key to move cursor left
                    KeyCode::Left => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                        }
                        queue!(self.stdout, MoveLeft(1))?;
                    }
                    // Handle Right arrow key to move cursor right
                    KeyCode::Right => {
                        if self.cursor_pos -1 < self.expression.len() {
                            self.cursor_pos += 1;
                        }
                        queue!(self.stdout, MoveRight(1))?;
                    }
                    // Exit on Esc key
                    KeyCode::Esc => break,
                    _ => {}
                },
                _ => {}
            }

            // Flush the stdout buffer to apply changes
            self.stdout.flush()?;
        }

        // Disable raw mode before exiting
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
