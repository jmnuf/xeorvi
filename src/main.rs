use std::{process, env, io, path, time};
use std::error::Error;
use std::io::{Write};

use crossterm::{self, QueueableCommand, cursor, terminal, event};
use crossterm::style::Stylize;
use whoami::fallible as whoami;
use is_executable::IsExecutable;

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
    // Environment
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let env_exes   = match query_env_exes() {
        Ok(list) => list,
        Err(_) => Vec::new(),
    };
    
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
    stdout.clear_term()?;
    
    while !should_quit {
        stdout.uflush()?;

        // Add extra lines when at the bottom of the terminal to make space for the "prompt"
        let (_x, y) = cursor::position().iu()?;
        if y == rows -1 {
            stdout.ubwrite("\n\n\n\n")?;
            stdout.uqueue(cursor::MoveUp(3))?;
        }
        // TODO: Move this to the handle_user_input function and redraw when user resizes window
        let top_bar_len = cols-(dir_name.chars().count()as u16)-4;
        stdout.uswrite(format!("╔┈{}/┈{:═<w$}", dir_name, "", w=top_bar_len as usize).cyan().on_black())?;
        stdout.uqueue(cursor::MoveDown(1))?;
        stdout.uqueue(cursor::MoveToColumn(0))?;
        // Activate raw mode temporarily to read the user input by hand a character at a time
        let (line, close_requested) = handle_user_input(
            &mut stdout,
            &username,
            &git_branch_name,
            &mut cols,
            &mut rows,
            &env_exes.iter().map(|(_, name)| name.clone()).collect(),
        )?;
        
        // Don't overlap with the design thingy
        stdout.ubwrite("\n")?;
        
        if close_requested {
            stdout.ubwrite("\n")?;
            stdout.uflush()?;
            should_quit = true;
            continue;
        }

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

        if cfg!(debug_assertions) {
            if uprog_name.to_lowercase() == "print-exes" {
                stdout.ubwrite(format!("Known executable files({}):\n", env_exes.len()))?;
                for (path, name) in env_exes.iter() {
                    stdout.ubwrite(format!(" - {}\n   @ {}\n", name, path.display()))?;
                }
                stdout.ubwrite("\n")?;
                continue;
            }
        }
        if cfg!(debug_assertions) {
            if uprog_name.to_lowercase() == "print-pp" {
                for (k, v) in std::env::vars_os() {
                    if k.to_string_lossy().to_string().to_lowercase() != "path" {
                        continue;
                    }
                    for path_str in v.to_string_lossy().to_string().split(if cfg!(windows) { ';' } else { ':' }) {
                        if path_str.is_empty() {
                            continue;
                        }
                        let path = path::Path::new(path_str);
                        stdout.ubwrite(format!("- {}\n", path_str))?;
                        if ! path.exists() {
                            stdout.ubwrite(" - Exists: false\n")?;
                            continue;
                        }
                        stdout.ubwrite(" - Exists: true\n")?;
                        if ! path.is_dir() {
                            stdout.ubwrite(" - IsFolder: false\n")?;
                            continue;
                        }
                        stdout.ubwrite(" - IsFolder: true\n")?;
                        if let Ok(entries) = path.read_dir() {
                            stdout.ubwrite(" - CanRead: true\n")?;
                            for entry_res in entries {
                                match entry_res {
                                    Ok(entry) => {
                                        stdout.ubwrite(format!(" - EntryPath: {}\n", entry.path().display()))?;
                                        let entry_path = entry.path();
                                        if !entry_path.is_file() {
                                            stdout.ubwrite(" - Entry.IsFile: false\n")?;
                                            continue;
                                        }
                                        stdout.ubwrite(" - Entry.IsFile: true\n")?;
                                        stdout.ubwrite(format!(" - Entry.IsExe: {}\n", entry_path.is_executable()))?;
                                    },
                                    Err(err) => {
                                        stdout.ubwrite(format!(" - EntryError: {}\n", err))?;
                                    },
                                };
                            }
                        } else {
                            stdout.ubwrite(" - CanRead: false\n")?;
                        }
                    }
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


fn handle_user_input(
    stdout: &mut io::Stdout,
    username: &str,
    git_branch_name: &str,
    cols: &mut u16,
    rows: &mut u16,
    env_exes: &Vec<String>
) -> Result<(String, bool), String> {
    terminal::enable_raw_mode().iu()?;
    
    let mut buf = String::new();
    let mut sgs = Vec::new();
    // TODO: Also include current directory stuff into suggestions
    let draw_line = move |stdout: &mut io::Stdout, cols: u16, usr_txt: &str, suggestions: &Vec<&String>| -> Result<(), String> {
        stdout.uqueue(cursor::MoveToColumn(0))?;
        stdout.uqueue(terminal::Clear(terminal::ClearType::CurrentLine))?;
        stdout.uswrite("╠┈".cyan().on_black())?;
        stdout.uswrite(format!("«{}»", username).white().on_black())?;
        if !git_branch_name.is_empty() {
            stdout.uswrite("┈Git(".red().on_black())?;
            stdout.uswrite(git_branch_name.white().on_black())?;
            stdout.uswrite(")".red().on_black())?;
        }
        stdout.ubwrite("∑◈ ")?;
        stdout.ubwrite(usr_txt)?;
        stdout.uqueue(cursor::SavePosition)?;
        stdout.uqueue(cursor::MoveDown(1))?;
        stdout.uqueue(cursor::MoveToColumn(0))?;
        stdout.uqueue(terminal::Clear(terminal::ClearType::CurrentLine))?;
        stdout.uswrite("╚═══════╝".cyan().on_black())?;
        
        if usr_txt.len() < 2 || suggestions.is_empty() {
            stdout.uswrite(" {}".dim().grey())?;
        } else {
            let (x, _) = cursor::position().iu()?;
            let mut it = suggestions.iter();
            if let Some(first) = it.next() {
                let mut x = x;
                if !usr_txt.starts_with(&**first) {
                    let s = format!(" {{{}", usr_txt);
                    x += s.chars().count() as u16;
                    stdout.uswrite(s.yellow())?;
                    stdout.ubwrite("|")?;
                    x += 1;
                    let s = first.chars().skip(usr_txt.len()).collect::<String>();
                    x += s.chars().count() as u16 + 1u16;
                    stdout.ubwrite(format!("{}}}", s))?;
                } else {
                    stdout.uswrite(" {".dim())?;
                    x += 2;
                    stdout.uswrite(format!("{}", first).yellow())?;
                    x += first.chars().count() as u16;
                    stdout.uswrite("}".dim())?;
                    x += 2;
                }
                while let Some(item) = it.next() {
                    let s = format!(" | {}", item);
                    x += s.chars().count() as u16;
                    if x >= cols {
                        break;
                    }
                    stdout.uswrite(s.dim())?;
                }
            }
        }
        stdout.uqueue(cursor::RestorePosition)?;
        stdout.uflush()?;
        Ok(())
    };

    for e in env_exes.iter() {
        sgs.push(e.clone());
    }
    draw_line(stdout, *cols, &buf, &sgs.iter().collect())?;
    let mut is_done = false;
    let mut last_suggestion = None;
    while !is_done {
        if event::poll(time::Duration::ZERO).iu()? {
            match event::read().iu()? {
                event::Event::Resize(new_cols_amt, new_rows_amt) => {
                    *cols = new_cols_amt;
                    *rows = new_rows_amt;
                    if cfg!(debug_assertions) {
                        stdout.ubwrite(format!("[DEBUG] Resized to: {}x{}\n", cols, rows))?;
                    }
                },
                event::Event::Key(event) => 'key_event_block: {
                    if event.kind != event::KeyEventKind::Press {
                        break 'key_event_block;
                    }
                    if !event.modifiers.is_empty() {
                        if event.modifiers == event::KeyModifiers::CONTROL {
                            if event.code == event::KeyCode::Backspace {
                                // Remove all whitespace ahead of characters cause I think that is what feels natural
                                'remove_whitespaced: {
                                    if let Some(mut c) = buf.pop() {
                                        while c.is_whitespace() {
                                            if let Some(nx) = buf.pop() {
                                                c = nx;
                                            } else {
                                                break 'remove_whitespaced;
                                            }
                                        }
                                        buf.push(c);
                                    }
                                }
                                // This should stop when hitting whitespace and symbols
                                let mut first = true;
                                while let Some(ch) = buf.pop() {
                                    if !ch.is_alphanumeric() {
                                        if !ch.is_whitespace() {
                                            if first {
                                                first = false;
                                                continue;
                                            }
                                            // Word boundaries should be kept I think
                                            buf.push(ch);
                                        }
                                        break;
                                    }
                                    first = false;
                                }
                            }
                            break 'key_event_block;
                        }
                        if event.modifiers != event::KeyModifiers::SHIFT {
                            break 'key_event_block;
                        }
                    }
                    match event.code {
                        event::KeyCode::Char(c) => { buf.push(c); },
                        event::KeyCode::Backspace => { let _ = buf.pop(); },
                        event::KeyCode::Enter => { is_done = true; },
                        event::KeyCode::Tab => {
                            if let Some(sg) = last_suggestion {
                                buf.clear();
                                buf.push_str(sg);
                                buf.push(' ');
                            }
                        },
                        _ => {},
                    };
                },
                // TODO: Implement pasting
                event::Event::Paste(_content) => {},
                _ => {},
            };
        }
        let sgs:Vec<_> = sgs.iter().filter(|name| name.starts_with(&buf)).collect();
        last_suggestion = sgs.first().map(|x| x.as_str());
        draw_line(stdout, *cols, &buf, &sgs)?;
    }
    draw_line(stdout, *cols, &buf, &Vec::new())?;
    stdout.uqueue(cursor::MoveDown(1))?;
    terminal::disable_raw_mode().iu()?;
    return Ok((buf, false));
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
    if cfg!(windows) {
        if path.to_lowercase().starts_with("c:/") {
            path.drain(..path.find("/").unwrap());
        }
    }
    if path.starts_with("/") {
        if cfg!(windows) {
            path = format!("C:{}", path);
        }
        let abs_dir = path::PathBuf::from(&path);
        if abs_dir.is_symlink() {
            drop(abs_dir);
            let (final_path, links_checked) = resolve_symlink(&new_path)?;
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

        if !abs_dir.is_dir() {
            return Err(format!("Can't CD into non-directory path: {}\n", new_path.display()));
        }
        // TODO: Need to handle the edge case when the user passed "absolute" path has relative
        // pathing inside of it i.e. /home/usr/personal/ecchi/../anime/./best-animes/evangelion
        return Ok(abs_dir);
    }
    if path == ".." {
        match new_path.parent() {
            Some(p) => return Ok(p.to_path_buf()),
            None => return Err(format!("Can't extract parent directory from {}\n", new_path.display())),
        };
    }
    while !path.is_empty() {
        if path.starts_with("./") {
            path.drain(..path.find("/").unwrap() + 1);
            continue;
        }
        // TODO: Handle when the parent is the root dir: "/" or "c:/"
        if path.starts_with("../") {
            path.drain(..path.find("/").unwrap() + 1);
            new_path = match new_path.parent() {
                Some(p) => p.to_path_buf(),
                None => return Err(format!("Can't extract parent directory from {}\n", new_path.display())),
            };
            continue;
        }
        if let Some(idx) = path.find("/") {
            let dir_name = String::from(path.drain(..idx).as_str());
            let _ = path.drain(..1); // Get rid of slash
            let tmp_path = new_path.join(&dir_name);
            if !tmp_path.exists() {
                return Err(format!("Can't find directory {}\n", tmp_path.display()));
            }
            new_path.push(dir_name);
            continue;
        }
        let dir_name = String::from(path.drain(..).as_str());
        let tmp_path = new_path.join(&dir_name);
        if !tmp_path.exists() {
            return Err(format!("Can't find directory {}\n", tmp_path.display()));
        }
        new_path.push(dir_name);
    }
    if new_path.is_symlink() {
        let (final_path, links_checked) = resolve_symlink(new_path.clone().as_path())?;
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


fn resolve_symlink(path: &path::Path) -> Result<(path::PathBuf, Vec<String>), String> {
    let mut final_path = path.to_path_buf();
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
    return Ok((final_path, links_checked));
}


fn query_env_exes() -> io::Result<Vec<(path::PathBuf, String)>> {
    let mut exes = Vec::new();
    for (key, val) in std::env::vars_os() {
        // Ignore anything that's not in the PATH variable
        if key.to_string_lossy().to_string().to_lowercase() != "path" {
            continue;
        }
        
        let mut paths = Vec::new();
        let val_string = val.to_string_lossy().to_string();
        #[cfg(target_family="windows")]
        const ENV_DELIM:char = ';';
        #[cfg(target_family="unix")]
        const ENV_DELIM:char = ':';
        
        if val_string.contains(ENV_DELIM) {
            for path in std::env::split_paths(&val) {
                if !path.exists() {
                    continue;
                }
                if path.is_file() {
                    paths.push(path);
                    continue;
                }
                if !path.is_dir() {
                    continue;
                }
                if let Ok(entries) = path.read_dir() {
                    for entry_res in entries {
                        if let Ok(entry) = entry_res {
                            let entry_path = entry.path();
                            if !entry_path.is_file() {
                                continue;
                            }
                            paths.push(entry_path);
                        }
                    }
                }
            }
        } else {
            let path = path::PathBuf::from(&val);
            let exists = path.exists();
            if !exists {
            } else if path.is_file() {
                paths.push(path);
            } else if path.is_dir() {
                if let Ok(entries) = path.read_dir() {
                    for entry_res in entries {
                        if !entry_res.is_ok() {
                            continue;
                        }
                        let entry_path = entry_res.unwrap().path();
                        if !entry_path.is_file() {
                            continue;
                        }
                        paths.push(entry_path);
                    }
                }
            }
        }
        let paths = paths;
        
        for path in paths.iter() {
            // TODO: Inspect symlinks I guess
            if path.is_symlink() {
                continue;
            }
            #[cfg(target_family="windows")]
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    // If a file don't end with .bat or .exe, it's not worth even checking if it's executable
                    // If you don't know why, the reason is "windows" trust me
                    if ext != "bat" && ext != "exe" {
                        continue;
                    }
                }
            }

            if !path.is_executable() {
                continue;
            }
            
            match path.file_name() {
                None => {},
                Some(os_name) => match os_name.to_str() {
                    None => {},
                    Some(name) => {
                        let name = name.to_string();
                        // To be honest, I'm not sure which one window's picks as priority .bat or .exe
                        // so best not to show the file extensions. Also if you have done this, that's
                        // kinda cringe bro. (I've done this, I need to clean my path variable)
                        #[cfg(target_family="windows")]
                        let name = match name.strip_suffix(".exe") {
                            None =>  name,
                            Some(stripped_name) => stripped_name.to_string(),
                        };
                        #[cfg(target_family="windows")]
                        let name = match name.strip_suffix(".bat") {
                            None =>  name,
                            Some(stripped_name) => stripped_name.to_string(),
                        };
                        exes.push((path.to_owned(), name));
                    },
                },
            };
        }
    }

    let mut names_added = Vec::new();
    let exes:Vec<_> = exes.into_iter().filter(|(_, name)| {
        if names_added.contains(name) {
            false
        } else {
            names_added.push(name.to_string());
            true
        }
    }).collect();
    
    return Ok(exes);
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
