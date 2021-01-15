use std::sync::mpsc;
use std::thread;

use inputbot::handle_input_events;
use inputbot::KeybdKey::{LControlKey, ZKey};

mod game_state;
mod piece_gen;
mod state;
mod sync_ppt;

use game_state::GameStateQueue;
use sync_ppt::{sync, Message, Notify};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    let (ktx, krx) = mpsc::sync_channel(1);
    let (ntx, nrx) = mpsc::channel();
    let (mtx, mrx) = mpsc::channel();

    thread::spawn(move || loop {
        krx.recv().unwrap();
        mtx.send(Message::Test).unwrap();
    });
    thread::spawn(move || {
        let mut game_state_queue = GameStateQueue::new();
        loop {
            let notify = nrx.recv().unwrap();
            match notify {
                Notify::Start(seed) => {
                    game_state_queue.push_new_game(seed as u32);
                }
                Notify::Sync(state) => {
                    println!("sync");
                    game_state_queue.update_by(state);
                }
            }
        }
    });

    ZKey.bind(move || {
        if LControlKey.is_pressed() {
            ktx.send(()).unwrap();
        }
    });
    thread::spawn(handle_input_events);

    if let Err(e) = sync(ntx, mrx) {
        eprintln!("An error occured: {}", e);
    }
}
