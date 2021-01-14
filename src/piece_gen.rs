// https://github.com/MinusKelvin/ultra-tas/blob/master/src/ppt1.rs
#[derive(Clone, Debug)]
pub enum Piece {
    S,
    Z,
    J,
    L,
    T,
    O,
    I,
}

#[derive(Clone, Debug)]
pub struct PieceGenerator {
    rng: u32,
    pub current_bag: Vec<Piece>,
}

impl PieceGenerator {
    pub fn new(seed: u32) -> Self {
        let mut this = PieceGenerator {
            rng: seed,
            current_bag: Vec::new(),
        };
        for _ in 0..1973 {
            this.rng();
        }
        this
    }

    fn rng(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(0x5D588B65).wrapping_add(0x269EC3);
        self.rng
    }
}

impl Iterator for PieceGenerator {
    type Item = Piece;
    fn next(&mut self) -> Option<Piece> {
        if let Some(piece) = self.current_bag.pop() {
            return Some(piece);
        }

        let mut bag = [
            Piece::S,
            Piece::Z,
            Piece::J,
            Piece::L,
            Piece::T,
            Piece::O,
            Piece::I,
        ];

        for i in 0..7 {
            let new_index = (((self.rng() >> 16) * (7 - i)) >> 16) + i;
            bag.swap(i as usize, new_index as usize);
        }

        bag.reverse();
        self.current_bag = bag.to_vec();

        self.current_bag.pop()
    }
}
