use std::env;

fn main() {
    for arg in env::args() {
        print!(" {}", arg);
    }
    println!();
}
