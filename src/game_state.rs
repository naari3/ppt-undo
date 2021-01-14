use crate::piece_gen::{Piece, PieceGenerator};
use crate::state::State;

#[derive(Clone)]
pub struct GameState {
    pub generator: PieceGenerator,
    pub queue: Vec<Piece>,
    state: State,
}

pub struct GameStateQueue {
    pub queue: Vec<GameState>,
}

impl GameState {
    pub fn new(seed: u32, state: State) -> Self {
        let generator = PieceGenerator::new(seed);
        let this = GameState {
            generator: PieceGenerator::new(seed),
            queue: generator.take(5).collect(),
            state,
        };
        this
    }

    pub fn consume_mino(&mut self) {
        // self.queue.remove(0);
        println!("old rng: {:?}", self.generator.rng);
        if let Some(next) = self.generator.next() {
            println!("next: {:?}", next);
            println!("new rng: {:?}", self.generator.rng);
            self.queue.push(next);
        }
        println!("queue: {:?}", self.queue)
    }
}

impl GameStateQueue {
    pub fn new() -> Self {
        GameStateQueue { queue: vec![] }
    }

    fn push_new(&mut self, game_state: GameState) {
        self.queue.push(game_state)
    }

    pub fn push_new_game(&mut self, seed: u32) {
        println!("seed updated: {}", seed);
        self.queue.clear();
        self.push_new(GameState::new(seed, State::new_blank()))
    }

    pub fn push_new_state(&mut self, state: State) {
        if let Some(last) = self.queue.last() {
            let mut new_game_state = last.clone();
            // TODO: Check current piece AND holded piece
            // TODO: Resolve can't consume rng (may be clone or ownership or wrong usage?)
            if new_game_state.state.current_piece != state.current_piece {
                new_game_state.consume_mino()
            }
            new_game_state.state = state;
            self.push_new(new_game_state)
        } else {
            println!("ERROR!! have not last game");
        }
    }

    pub fn update_by(&mut self, state: State) {
        if let Some(last) = self.queue.last() {
            if last.state != state {
                self.push_new_state(state)
            }
        } else {
            println!("ERROR!! have not last game");
        }
    }
}
