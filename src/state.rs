use process_memory::{Architecture, DataMember, Memory};
use winapi::um::winnt::HANDLE;

use std::convert::TryInto;

use crate::Result;

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub columns: Vec<Vec<i16>>,
    pub current_piece: Option<u16>,
    pub hold: Option<u16>,
    pub next_queue: Vec<u16>,
}

impl State {
    pub fn new_from_proc(handle: HANDLE) -> Result<Self> {
        let handle = (handle, Architecture::from_native());
        let current_piece =
            DataMember::<i32>::new_offset(handle, vec![0x140598A20, 0x38, 0x3c8, 0x8]).read()?;
        let current_piece = if current_piece >= 0 {
            Some(current_piece as u16)
        } else {
            None
        };

        let hold_address =
            DataMember::<usize>::new_offset(handle, vec![0x140598A20, 0x38, 0x3D0]).read()?;
        let hold = match hold_address {
            0 => None,
            _ => {
                Some(DataMember::<u32>::new_offset(handle, vec![hold_address + 0x8]).read()? as u16)
            }
        };

        let board_address = DataMember::<usize>::new_offset(
            handle,
            vec![0x140461B20, 0x378, 0xC0, 0x10, 0x3C0, 0x18],
        )
        .read()?;
        let columns_addresses =
            DataMember::<[usize; 10]>::new_offset(handle, vec![board_address]).read()?;
        let mut columns: Vec<Vec<i16>> = Vec::new();
        for column_address in columns_addresses.iter() {
            let pieces = DataMember::<[i32; 40]>::new_offset(handle, vec![*column_address]);
            columns.push(
                pieces
                    .read()?
                    .to_vec()
                    .iter()
                    .map(|&c| c as i16)
                    .collect::<Vec<_>>(),
            );
        }

        let next_queue =
            DataMember::<[u32; 5]>::new_offset(handle, vec![0x140461B20, 0x378, 0xB8, 0x15C])
                .read()?
                .to_vec()
                .into_iter()
                .map(|f| f as u16)
                .collect::<Vec<_>>();

        Ok(Self {
            columns,
            current_piece,
            hold,
            next_queue,
        })
    }

    pub fn new_blank() -> Self {
        State {
            columns: vec![],
            current_piece: None,
            hold: None,
            next_queue: vec![],
        }
    }
}

pub fn write_current_piece(handle: HANDLE, piece: u16) -> Result<()> {
    let handle = (handle, Architecture::from_native());
    DataMember::<i32>::new_offset(handle, vec![0x140598A20, 0x38, 0x3c8, 0x8])
        .write(&(piece as i32))?;
    Ok(())
}

pub fn write_hold(handle: HANDLE, piece: u16) -> Result<()> {
    let handle = (handle, Architecture::from_native());
    DataMember::<i32>::new_offset(handle, vec![0x140598A20, 0x38, 0x3d0, 0x8])
        .write(&(piece as i32))?;
    Ok(())
}

pub fn write_board(handle: HANDLE, columns: Vec<Vec<i16>>) -> Result<()> {
    let handle = (handle, Architecture::from_native());
    let board_address =
        DataMember::<usize>::new_offset(handle, vec![0x140461B20, 0x378, 0xC0, 0x10, 0x3C0, 0x18])
            .read()?;
    let columns_addresses =
        DataMember::<[usize; 10]>::new_offset(handle, vec![board_address]).read()?;
    for (column_address, column) in columns_addresses.iter().zip(columns.iter()) {
        let column: &[i32; 40] = &column
            .iter()
            .map(|&n| n as i32)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        DataMember::<[i32; 40]>::new_offset(handle, vec![*column_address])
            .write(column)
            .unwrap();
    }
    Ok(())
}

pub fn get_seed(handle: HANDLE) -> Result<u16> {
    let handle = (handle, Architecture::from_native());
    let seed = DataMember::<u16>::new_offset(handle, vec![0x140463FD8, 0x78]).read()?;
    Ok(seed)
}

pub fn get_is_current_piece_active(handle: HANDLE) -> Result<bool> {
    let handle = (handle, Architecture::from_native());
    let seed =
        DataMember::<bool>::new_offset(handle, vec![0x140590F70, 0x20, 0x3C8, 0x1C]).read()?;
    Ok(!seed)
}
