//! Summarise a RimWorld Player.log: cargo run -p circinus-core --example playerlog -- <Player.log>
fn main() {
    let path = std::env::args().nth(1).expect("path to Player.log");
    let text = std::fs::read_to_string(&path).expect("read");
    let r = circinus_core::playerlog::parse(&text);
    println!("{}", serde_json::to_string_pretty(&r).unwrap());
}
