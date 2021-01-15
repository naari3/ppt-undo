use std::sync::mpsc;
use std::thread;

mod game_state;
mod piece_gen;
mod state;
mod sync_ppt;

use game_state::GameStateQueue;
use sync_ppt::{sync, Notify};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut game_state_queue = GameStateQueue::new();
        loop {
            let notify = rx.recv().unwrap();
            match notify {
                Notify::Start(seed) => {
                    game_state_queue.push_new_game(seed as u32);
                }
                Notify::Sync(state) => {
                    game_state_queue.update_by(state);
                }
            }
        }
    });
    if let Err(e) = sync(tx) {
        eprintln!("An error occured: {}", e);
    }
}
