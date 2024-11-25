use std::process::{ExitCode, Command};
use std::fs;
use std::io;
use std::path::PathBuf;

const BIN_PATH:&'static str = "./bin";
const EXE_NAME:&'static str = "xeorvi";

fn get_args() -> (String, Vec<String>) {
    let mut args = std::env::args();
    let program_name = args.next().expect("Argument 0 should always be the program name");
    
    return (program_name, args.collect());
}

fn main() -> ExitCode {
    let (program_name, args) = get_args();

    let release_build;
    let build_path;
    let command = if let Some(a) = args.get(0) {
        if a == "release" || a == "-r" {
            release_build = true;
            build_path = "./target/release";
            ["cargo", "build", "--release"]
        } else if a == "debug" || a == "-d" {
            release_build = false;
            build_path = "./target/debug";
            ["cargo", "build", ""]
        } else {
            eprintln!("[ERROR] Invalid option passed");
            println!("Usage: {} [(release|-r) | (debug|-d)]", program_name);
            return ExitCode::FAILURE;
        }
    } else {
        release_build = true;
        build_path = "./target/release";
        println!("[INFO] Build type not specified, defaulting to release mode");
        ["cargo", "build", "--release"]
    };
    
    let st = {
        let mut cmd = None;
        for a in command.iter() {
            if a.is_empty() {
                continue;
            }
            match cmd {
                None => {
                    cmd = Some(Command::new(a));
                    print!("[CMD] {}", a);
                },
                Some(ref mut cmd) => {
                    cmd.arg(a);
                    print!(" {}", a);
                },
            };
        }
        println!();

        cmd.unwrap().status()
    };

    match st {
        Err(err) => {
            eprintln!("[ERROR] {}", err);
            return ExitCode::FAILURE;
        },
        Ok(status) => {
            if status.success() {
                println!("[INFO] Compiled succesfully");
            } else {
                eprintln!("[ERROR] Failed to compile");
                return ExitCode::FAILURE;
            }
        },
    };

    println!("[INFO] Attempting to move executable from target folder onto bin directory...");
    
    match PathBuf::from(BIN_PATH).try_exists() {
        Err(err) => {
            eprintln!("[ERROR] {}", err);
            return ExitCode::FAILURE;
        },
        Ok(exists) => {
            if !exists {
                println!("[INFO] Attempting to create bin directory...");
                match fs::create_dir_all(BIN_PATH) {
                    Ok(_) => println!("[INFO] Created bin directory..."),
                    Err(err) => {
                        eprintln!("[ERROR] {}", err);
                        return ExitCode::FAILURE;
                    },
                };
            }
        },
    };

    let file_path = if cfg!(windows) {
        if release_build {
            format!("{}/{}.exe", BIN_PATH, EXE_NAME)
        } else {
            format!("{}/{}-dbg.exe", BIN_PATH, EXE_NAME)
        }
    } else {
        if release_build {
            format!("{}/{}", BIN_PATH, EXE_NAME)
        } else {
            format!("{}/{}-dbg", BIN_PATH, EXE_NAME)
        }
    };

    println!("[INFO] Removing previous exe if exists...");
    match fs::remove_file(&file_path) {
        Ok(_) => {},
        Err(err) => {
            match err.kind() {
                io::ErrorKind::NotFound => {},
                _ => {
                    eprintln!("[ERROR] {}", err);
                    return ExitCode::FAILURE;
                },
            };
        },
    };

    let old_file_path = if cfg!(windows) {
        format!("{}/{}.exe", build_path, EXE_NAME)
    } else {
        format!("{}/{}", build_path, EXE_NAME)
    };

    println!("[INFO] Attempting move {} -> {}", old_file_path, file_path);
    match fs::rename(&old_file_path, &file_path) {
        Err(err) => {
            eprintln!("[ERROR] {}", err);
            return ExitCode::FAILURE;
        },
        Ok(_) => {
            println!("[INFO] {} -> {}", old_file_path, file_path);
        },
    };

    return ExitCode::SUCCESS;
}
