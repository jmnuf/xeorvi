use std::{process, env, io, path, time};
use std::error::Error;
use std::io::{Write, BufRead};

use crossterm::{self, QueueableCommand, cursor, terminal, event};
use crossterm::style::Stylize;
use whoami::fallible as whoami;

fn main() -> process::ExitCode {
    let mut args = env::args();
    let program_name:String = args.next().expect("Program name should always be argument 0 of the program");
    match run(&program_name, args) {
        Ok(_) => {
            clean_up();
            process::ExitCode::SUCCESS
        },
        Err(e) => {
            clean_up();
            // The most amazing error reporting you've ever seen
            eprintln!("[ERROR] {}", e);
            process::ExitCode::FAILURE
        },
    }
}

fn clean_up() {
    // I got no clue if I'll really be using raw mode eventually but boilerplate is done
    match terminal::is_raw_mode_enabled() {
        Err(_) => {},
        Ok(enabled) => {
            if enabled {
                // If we fail, good luck have fun
                match terminal::disable_raw_mode() {
                    Ok(_) => {},
                    Err(e) => {
                        eprintln!("[ERROR] Failed to close raw mode: {}", e);
                    },
                };
            }
        },
    };
}

#[allow(dead_code)]
enum CmdChain {
    And(process::Command),
    Pipe(process::Command),
}

struct CmdReq {
    start: process::Command,
    chain: Option<Vec<CmdChain>>,
}

fn run(program_name: &str, _args: env::Args) -> Result<(), String> {
    // Clean Environment
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    stdout.clear_term()?;
    
    // Setup base data
    let mut should_quit = false;
    let quit_commands = ["kys", "exit", "quite", "q", "kindness"];
    let (mut dir_path, mut dir_name) = query_current_directory_name()?;
    let mut git_branch_name = match query_git_branch_name() {
        Ok(Some(name)) => name,
        _ => String::new(),
    };
    let username = match query_username() {
        Ok(x) => x,
        Err(e) => {
            eprintln!("[ERROR] Failed to get username, defaulting to 'anon': {}", e);
            String::from("anon")
        },
    };

    // Setup environment data
    let (mut cols, mut rows) = terminal::size().iu()?;
    stdout.uqueue(cursor::MoveTo(0, 0))?;
    
    while !should_quit {
        stdout.uflush()?;
        while event::poll(time::Duration::ZERO).iu()? {
            match event::read().iu()? {
                event::Event::Resize(new_cols_amt, new_rows_amt) => {
                    cols = new_cols_amt;
                    rows = new_rows_amt;
                    stdout.ubwrite(format!("[DEBUG] Resized to: {}x{}", cols, rows))?;
                },
                _ => {},
            };
        }
        // Add extra lines when at the bottom of the terminal to make space for the "prompt"
        let (_x, y) = cursor::position().iu()?;
        if y == rows -1 {
            stdout.ubwrite("\n\n\n\n")?;
            stdout.uqueue(cursor::MoveUp(3))?;
        }
        // TODO: Actually move the prompt to a separate function just to make this function a bit shorter?
        let top_bar_len = cols-(dir_name.chars().count()as u16)-4;
        stdout.uswrite(format!("╔┈{}/┈{:═<w$}", dir_name, "", w=top_bar_len as usize).cyan().on_black())?;
        stdout.uqueue(cursor::MoveDown(1))?;
        stdout.uqueue(cursor::MoveToColumn(0))?;
        stdout.uswrite("╠┈".cyan().on_black())?;
        stdout.uswrite(format!("«{}»", username).white().on_black())?;
        if !git_branch_name.is_empty() {
            stdout.uswrite("┈Git(".red().on_black())?;
            stdout.uswrite(git_branch_name.clone().white().on_black())?;
            stdout.uswrite(")".red().on_black())?;
        }
        stdout.ubwrite("∑◈ ")?;
        stdout.uqueue(cursor::SavePosition)?;
        stdout.uqueue(cursor::MoveDown(1))?;
        stdout.uqueue(cursor::MoveToColumn(0))?;
        stdout.uswrite("╚═══════╝".cyan().on_black())?;
        stdout.uswrite(" TODO: Implement auto complete options that should go here".dim().grey())?;
        stdout.uqueue(cursor::RestorePosition)?;
        stdout.uflush()?;

        // TODO: Instead of just reading a line, maybe switch to raw mode and handle all input manually
        let mut line = String::with_capacity(64);
        let _bytes_read = io::stdin().lock().read_line(&mut line).iu()?;
        let line = line.trim().to_string();
        // Don't overlap with the design thingy
        stdout.ubwrite("\n")?;

        if line.is_empty() {
            continue;
        }
        
        if quit_commands.contains(&line.to_lowercase().as_str()) {
            should_quit = true;
            continue;
        }

        let req = match parse_user_input(line) {
            Ok(x) => x,
            Err(err) => {
                stderr.uswrite("[uERROR]".red())?;
                stderr.uswrite(format!(" {}\n", err))?;
                continue;
            },
        };
        let uprog_name = req.start.get_program().to_string_lossy().to_string();
        if req.chain.is_some() {
            stderr.uswrite("[tERROR]".red())?;
            stderr.uswrite(" Chaining/piping commands is not supported, yet!\n")?;
            continue;
        }

        if uprog_name.to_lowercase() == "cd" || uprog_name.to_lowercase() == "chdir" {
            let uargs:Vec<_> = req.start.get_args().collect();
            if let Some(path) = uargs.get(0) {
                match parse_path(&dir_path, &path.to_string_lossy().to_string()) {
                    Err(err) => {
                        stderr.uswrite("[?ERROR]".red())?;
                        stderr.ubwrite(format!(" {}", err))?;
                    },
                    Ok(path) => {
                        match env::set_current_dir(&path) {
                            Ok(_) => dir_path = path,
                            Err(err) => {
                                stderr.uswrite("[sERROR]".red())?;
                                stderr.ubwrite(format!(" Failed to switch dir: {}\n", err))?;
                            },
                        };
                        match dir_path.file_name() {
                            Some(x) => dir_name = x.to_string_lossy().to_string(),
                            None => {
                                stderr.uswrite("[ERROR]".red())?;
                                stderr.ubwrite(" Rust failed to get directory name separated\n")?;
                            },
                        };
                        git_branch_name = match query_git_branch_name() {
                            Ok(Some(name)) => name,
                            _ => String::new(),
                        };
                    },
                };
            } else {
                stdout.uswrite(format!("{}\n", dir_path.display()))?;
            }
            continue;
        }

        if cfg!(debug_assertions) {
            if uprog_name.to_lowercase() == "print-env" {
                stdout.ubwrite("Commands inherited env vars:\n")?;
                let env_vars:Vec<_> = req.start.get_envs().collect();
                if env_vars.is_empty() {
                    stdout.ubwrite(":> None\n")?;
                }
                for (vname_os, maybe_vvalue_os) in env_vars.iter() {
                    stdout.ubwrite(":> ")?;
                    stdout.uswrite(format!("{:?} -> {:?}\n", vname_os, maybe_vvalue_os))?;
                }
                stdout.uswrite(format!("{} env vars:\n", program_name))?;
                let env_vars:Vec<_> = env::vars_os().collect();
                if env_vars.is_empty() {
                    stdout.ubwrite(":> None\n")?;
                }
                for (key, val) in env::vars_os() {
                    stdout.ubwrite(":> ")?;
                    stdout.uswrite(format!("{:?} -> {:?}\n", key, val))?;
                }
                continue;
            }
        }

        if uprog_name.to_lowercase() == "echo" {
            let mut first = true;
            for uarg in req.start.get_args().map(|x| x.to_string_lossy()) {
                if first {
                    stdout.uswrite(uarg)?;
                    first = false;
                } else {
                    stdout.uswrite(format!(" {}", uarg))?;
                }
            }
            stdout.ubwrite("\n")?;
            continue;
        }

        if uprog_name.to_lowercase() == "cls" || uprog_name.to_lowercase() == "clear" {
            stdout.clear_term()?;
            stdout.uqueue(cursor::MoveTo(0, 0))?;
            continue;
        }

        if cfg!(debug_assertions) {
            stdout.ubwrite(format!("[uCMD] {:?}\n", req.start))?;
        }
        stdout.uflush()?;

        let mut req = req;
        match req.start.spawn() {
            Ok(mut child) => match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        match status.code() {
                            Some(code) => stdout.uswrite(format!("t :: exit code was {}\n", code))?,
                            None => stdout.uswrite(format!("t :: program closed by signal\n"))?,
                        };
                    }
                },
                Err(err) => {
                    stderr.uswrite("[cERROR]".red())?;
                    stderr.ubwrite(format!(" {}\n", err))?;
                },
            },
            Err(err) => {
                stderr.uswrite("[cERROR]".red())?;
                stderr.ubwrite(format!(" {}\n", err))?;
            },
        };
    }

    Ok(())
}


// TODO: Test this monstrosity
fn parse_user_input(input: String) -> Result<CmdReq, String> {
    let mut buffer = String::new();
    let mut command:Option<CmdReq> = None;
    let mut chars = input.chars();
    let mut quotes = None;
    while let Some(ch) = chars.next() {
        if let Some(qch) = quotes {
            if ch == qch {
                quotes = None;
                continue;
            }
            // TODO: Handle proper escaping
            if ch == '\\' {
                if let Some(nch) = chars.next() {
                    buffer.push(nch);
                    continue;
                }
            }
            buffer.push(ch);
            continue;
        }
        match ch {
            '&' | '|' => return Err(format!("[TODO] Parsing {} is not supported yet", ch)),
            '"' | '\'' => quotes = Some(ch),
            ' ' => match command {
                None => {
                    command = Some(CmdReq {
                        start: process::Command::new(&buffer),
                        chain: None
                    });
                    buffer.clear();
                },
                Some(ref mut req) => {
                    if let Some(ref mut chained) = req.chain {
                        match chained.last_mut() {
                            Some(CmdChain::And(cmd)) => cmd.arg(&buffer),
                            Some(CmdChain::Pipe(cmd)) => cmd.arg(&buffer),
                            _ => unreachable!(),
                        };
                    } else {
                        req.start.arg(&buffer);
                    }
                    buffer.clear();
                }
            },
            _ => buffer.push(ch),
        };
    }
    
    if !buffer.is_empty() {
        match command {
            Some(ref mut req) => {
                if let Some(ref mut chained) = req.chain {
                    match chained.last_mut() {
                        Some(CmdChain::And(ref mut cmd)) => cmd.arg(buffer),
                        Some(CmdChain::Pipe(ref mut cmd)) => cmd.arg(buffer),
                        _ => unreachable!(),
                    };
                } else {
                    req.start.arg(buffer);
                }
            },
            None => {
                command = Some(CmdReq {
                    start: process::Command::new(buffer),
                    chain: None
                });
            },
        };
    }
    
    return match command {
        Some(x) => Ok(x),
        None => Err(format!("Unknown syntax or command: {}", input)),
    };
}

// TODO: Test this abomination
fn parse_path(cwd: &path::PathBuf, path: &str) -> Result<path::PathBuf, String> {
    let mut new_path = cwd.clone();
    let mut path = path.replace("\\", "/");
    if path.starts_with("/") {
        // TODO: Handle absolute paths
        return Err(format!("Unable to change directory to `{}` as absolute paths are not supported, yet!", path));
    }
    if path == ".." {
        match new_path.parent() {
            Some(p) => return Ok(p.to_path_buf()),
            None => return Err(format!("Can't extract parent directory from {}", new_path.display())),
        };
    }
    while !path.is_empty() {
        if path.starts_with("./") {
            path.drain(..path.find("/").unwrap() + 1);
            continue;
        }
        if path.starts_with("../") {
            path.drain(..path.find("/").unwrap() + 1);
            new_path = match new_path.parent() {
                Some(p) => p.to_path_buf(),
                None => return Err(format!("Can't extract parent directory from {}", new_path.display())),
            };
            continue;
        }
        if let Some(idx) = path.find("/") {
            let dir_name = String::from(path.drain(..idx).as_str());
            let _ = path.drain(..1); // Get rid of slash
            let tmp_path = new_path.join(&dir_name);
            if !tmp_path.exists() {
                return Err(format!("Can't find directory {}", tmp_path.display()));
            }
            new_path.push(dir_name);
            continue;
        }
        let dir_name = String::from(path.drain(..).as_str());
        let tmp_path = new_path.join(&dir_name);
        if !tmp_path.exists() {
            return Err(format!("Can't find directory {}", tmp_path.display()));
        }
        new_path.push(dir_name);
    }
    if new_path.is_symlink() {
        let mut final_path = new_path.clone();
        let mut links_checked = Vec::new();
        links_checked.push(format!("{}", final_path.display()));
        while final_path.is_symlink() {
            match final_path.read_link() {
                Err(err) => return Err(format!("Failed to read symlink target: {}\n", err)),
                Ok(linked_path) => {
                    links_checked.push(format!("{}", linked_path.display()));
                    final_path = linked_path;
                },
            };
        }
        if !final_path.is_dir() {
            let mut error = String::from("Can't CD onto non-directory path: {}\n");
            let mut prev = String::with_capacity(0);
            for (i, cur) in links_checked.iter().enumerate() {
                if i == 0 {
                    prev = cur.clone();
                    continue;
                }
                error = format!("{}  `{}` -> `{}`\n", error, prev, cur);
            }
            return Err(error);
        }
        return Ok(final_path);
    }
    if !new_path.is_dir() {
        return Err(format!("Can't CD into non-directory path: {}\n", new_path.display()));
    }
    return Ok(new_path);
}



// TODO: Is it better to do a search for the .git folder? Did it this way cause it was the easiest and "it just works" - Tod Howard
fn query_git_branch_name() -> io::Result<Option<String>> {
    let mut args = Vec::new();
    args.push("branch".to_string());
    args.push("--show-current".to_string());
    return process::Command::new("git")
        .arg("branch")
        .arg("--show-current")
        .output()
        .map(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });
}

// Not sure if to keep whoami so for now it's placed in this little isolation box we call a function.
fn query_username() -> Result<String, io::Error> {
    return whoami::username();
}

fn query_current_directory_name() -> Result<(path::PathBuf, String), String> {
    let dir = env::current_dir().iu()?;
    let file_name = match dir.file_name() {
        Some(f) => f.to_string_lossy().to_string(),
        None => return Err(format!("Unabled to get the current directory's name. This might be because path ends with `..`, but truly IDK why gomena-sorry :(")),
    };
    return Ok((dir, file_name));
}











/**
 * ===================================
 * | <Traits Section>
 * | Viewer discretion is adviced
 * -----------------------------------
 */

trait TermClearer<'a> {
    fn clear_term(&'a mut self) -> Result<&'a mut Self, String>;
}
impl<T: QueueableCommand> TermClearer<'_> for T {
    fn clear_term(&mut self) -> Result<&mut Self, String> {
        self.queue(terminal::Clear(terminal::ClearType::All)).iu()
    }
}

trait UWrite: Write {
    fn uwrite(&mut self, buf: &[u8]) -> Result<usize, String>;
    fn uswrite(&mut self, buf: impl std::fmt::Display) -> Result<usize, String>;
    fn ubwrite(&mut self, buf: impl std::convert::AsRef<[u8]>) -> Result<usize, String>;
    fn uflush(&mut self) -> Result<(), String>;
}
impl<T: Write> UWrite for T {
    fn uflush(&mut self) -> Result<(), String> {
        self.flush().iu()
    }
    fn uwrite(&mut self, buf: &[u8]) -> Result<usize, String> {
        self.write(buf).iu()
    }
    fn uswrite(&mut self, buf: impl std::fmt::Display) -> Result<usize, String> {
        self.uwrite(format!("{}", buf).as_bytes())
    }
    fn ubwrite(&mut self, buf: impl std::convert::AsRef<[u8]>) -> Result<usize, String> {
        self.uwrite(buf.as_ref())
    }
}

trait UQueueable<'a>: QueueableCommand {
    fn uqueue(&'a mut self, cmd: impl crossterm::Command) -> Result<&'a mut Self, String>;
}
impl<T: QueueableCommand> UQueueable<'_> for T {
    fn uqueue(&mut self, cmd: impl crossterm::Command) -> Result<&mut Self, String> {
        self.queue(cmd).iu()
    }
}


// This is literally just to have a shorthand way of bubbling the errors up to main since I use Result<(), String>
trait IntoU<T> {
    fn iu(self) -> T;
}

impl<TV, TE: Error> IntoU<Result<TV, String>> for Result<TV, TE> {
    fn iu(self) -> Result<TV, String> {
        self.map_err(|e| format!("{}", e))
    }
}
