use std::sync::mpsc;
use std::sync::{Arc, Mutex};
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
    let gsq = Arc::new(Mutex::new(GameStateQueue::new()));
    let (ktx, krx) = mpsc::sync_channel(1);
    let (ntx, nrx) = mpsc::channel();
    let (mtx, mrx) = mpsc::channel();

    let gsqq1 = Arc::clone(&gsq);

    thread::spawn(move || loop {
        krx.recv().unwrap();
        let mut gsq = gsqq1.lock().unwrap();
        if gsq.queue.len() >= 2 {
            gsq.queue.pop();
            if let Some(gs) = gsq.queue.pop() {
                mtx.send(Message::Undo(gs)).unwrap();
            }
        }
    });

    let gsqq2 = Arc::clone(&gsq);
    thread::spawn(move || loop {
        {
            let notify = nrx.recv().unwrap();
            let mut gsq = gsqq2.lock().unwrap();
            match notify {
                Notify::Start(seed) => {
                    gsq.push_new_game(seed as u32);
                }
                Notify::Sync(state) => {
                    println!("sync");
                    gsq.update_by(state);
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
