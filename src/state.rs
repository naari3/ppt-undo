use process_memory::{Architecture, DataMember, Memory};
use winapi::um::winnt::HANDLE;

#[derive(Clone, Debug)]
struct State {
    pub columns: Vec<Vec<i16>>,
    pub current_piece: Option<u16>,
    pub hold: Option<u16>,
}

impl State {
    pub fn new_from_proc(handle: HANDLE) -> Self {
        let handle = (handle, Architecture::from_native());
        let current_piece_addr =
            DataMember::<i32>::new_offset(handle, vec![0x140461B20, 0x378, 0x40, 0x140, 0x110]);
        Self {
            columns: vec![vec![]],
            current_piece: Some(1),
            hold: Some(1),
        }
    }
}
