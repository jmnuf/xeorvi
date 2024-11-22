use std::process::{ExitCode, Command};
use std::fs;
use std::io;
use std::path::PathBuf;

// TODO: Actually also take into account non-release builds cause this probably will get slow
const CARGO_BUILD_PATH:&'static str = "./target/release";
const BIN_PATH:&'static str = "./bin";
const EXE_NAME:&'static str = "xeorvi";

fn main() -> ExitCode {
    let command = ["cargo", "build", "--release"];

    let st = {
        let mut cmd = None;
        for a in command.iter() {
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
        format!("{}/{}.exe", BIN_PATH, EXE_NAME)
    } else {
        format!("{}/{}", BIN_PATH, EXE_NAME)
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
        format!("{}/{}.exe", CARGO_BUILD_PATH, EXE_NAME)
    } else {
        format!("{}/{}", CARGO_BUILD_PATH, EXE_NAME)
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
