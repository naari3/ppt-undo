use mpsc::Sender;
use std::os::windows::ffi::OsStringExt;
use std::sync::mpsc;
use std::thread;
use winapi::um::debugapi::*;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::CloseHandle;
use winapi::um::memoryapi::{ReadProcessMemory, WriteProcessMemory};
use winapi::um::minwinbase::*;
use winapi::um::processthreadsapi::{GetThreadContext, OpenProcess, OpenThread, SetThreadContext};
use winapi::um::psapi::{EnumProcessModules, EnumProcesses, GetModuleBaseNameW};
use winapi::um::winbase::*;
use winapi::um::winnt::*;

mod state;
use state::{get_seed, State};

mod game_state;
mod piece_gen;
use game_state::GameStateQueue;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub enum Notify {
    Start(u16),
    Sync(State),
}

macro_rules! w {
    ($f:ident($($content:tt)*)) => {
        match $f($($content)*) {
            0 => {
                eprintln!(
                    "{} (line {}) failed with error code {}",
                    stringify!(f), line!(), GetLastError()
                );
                panic!();
            }
            v => v
        }
    };
}

pub fn sync(notifier: Sender<Notify>) -> Result<()> {
    let pid = find_ppt_process()?.unwrap_or_else(|| {
        println!("No such process");
        std::process::exit(1)
    });

    unsafe {
        w!(DebugActiveProcess(pid));

        let dbg_event = wait_for_event();
        if dbg_event.dwDebugEventCode != CREATE_PROCESS_DEBUG_EVENT {
            panic!("first debug event should've been a CREATE_PROCESS_DEBUG_EVENT");
        }
        let process = dbg_event.u.CreateProcessInfo().hProcess;
        let mut tid = dbg_event.dwThreadId;
        let mut continue_kind = DBG_EXCEPTION_NOT_HANDLED;

        play(pid, &mut tid, &mut continue_kind, process, notifier)?;

        ContinueDebugEvent(pid, tid, continue_kind);

        w!(DebugActiveProcessStop(pid));
    }

    Ok(())
}

fn play(
    pid: u32,
    tid: &mut u32,
    continue_kind: &mut u32,
    process: HANDLE,
    notifier: Sender<Notify>,
) -> Result<()> {
    const INSTRUCTION_ADDRESS: u64 = 0x14025B8CC;
    // const RNG_INITIAL_ADDRESS: u64 = 0x14003F87F;

    unsafe {
        // breakpoint for initial RNG
        // let thread = breakpoint(pid, tid, continue_kind, process, RNG_INITIAL_ADDRESS)?;

        // let mut regs = CONTEXT::default();
        // regs.ContextFlags = CONTEXT_ALL;
        // w!(GetThreadContext(thread, &mut regs));

        // // Game expects rng to be in rax
        // // Game shifts down by 16 bits so the seed is only 16 bits large
        // // Altering this line allows impossible seeds to be used (such as the one for the 19.71s
        // // sprint TAS), which lets us see what could have been if sega hadn't clipped the seed
        // // space.
        // println!("current seed: {}", regs.Rax);
        // // regs.Rax = seed & 0xFFFF;
        // w!(SetThreadContext(thread, &regs));

        // CloseHandle(thread);

        let mut latest_seed = 0u16;
        let mut latest_state = State::new_blank();
        let mut latest_piece: Option<u16> = None;
        let mut latest_hold: Option<u16> = None;
        let mut latest_queue: Vec<u16> = vec![];

        loop {
            // breakpoint for input system
            // println!("asd");
            let thread = breakpoint(pid, tid, continue_kind, process, INSTRUCTION_ADDRESS)?;

            // let mut regs = CONTEXT::default();
            // regs.ContextFlags = CONTEXT_ALL;
            // if GetThreadContext(thread, &mut regs) == 0 {
            //     panic!();
            // }
            // // game expects input bitfield to be in rbx
            // regs.Rbx = input;
            // if SetThreadContext(thread, &regs) == 0 {
            //     panic!();
            // }
            if let Ok(tmp_seed) = get_seed(process) {
                if tmp_seed != latest_seed {
                    notifier.send(Notify::Start(tmp_seed))?;
                    latest_seed = tmp_seed;
                    latest_state = State::new_blank();
                    latest_piece = None;
                };
            };
            let state = State::new_from_proc(process);
            if let Ok(state) = state {
                if state != latest_state {
                    if state.current_piece != None {
                        if state.current_piece != latest_piece && state.hold != latest_hold
                            || state.next_queue != latest_queue
                        {
                            println!("state.current_piece: {:?}", state.current_piece);
                            notifier.send(Notify::Sync(state.clone()))?;
                            latest_piece = state.current_piece;
                            latest_hold = state.hold;
                            latest_queue = state.next_queue.clone();
                        }
                    }
                    latest_state = state;
                }
            };

            CloseHandle(thread);
            // advance past breakpoint
            if step(pid, tid, continue_kind).is_err() {
                break;
            }
        }
    }

    Ok(())
}

unsafe fn wait_for_event() -> DEBUG_EVENT {
    let mut event = Default::default();
    WaitForDebugEvent(&mut event, INFINITE);
    event
}

fn breakpoint(
    pid: u32,
    tid: &mut u32,
    continue_kind: &mut u32,
    process: HANDLE,
    address: u64,
) -> Result<HANDLE> {
    unsafe {
        let mut original = 0u8;
        let mut rw = 0;
        w!(ReadProcessMemory(
            process,
            address as *mut _,
            &mut original as *mut _ as *mut _,
            1,
            &mut rw,
        ));

        w!(WriteProcessMemory(
            process,
            address as *mut _,
            &0xCC as *const _ as *const _,
            1,
            &mut rw,
        ));

        loop {
            // dbg!(*continue_kind);
            w!(ContinueDebugEvent(pid, *tid, *continue_kind));
            let event = wait_for_event();
            *tid = event.dwThreadId;
            if event.dwDebugEventCode != EXCEPTION_DEBUG_EVENT {
                if event.dwDebugEventCode == EXIT_PROCESS_DEBUG_EVENT {
                    panic!("ppt exited");
                }
                *continue_kind = DBG_EXCEPTION_NOT_HANDLED;
                continue;
            }

            let info = &event.u.Exception().ExceptionRecord;
            if info.ExceptionCode != EXCEPTION_BREAKPOINT {
                *continue_kind = DBG_EXCEPTION_NOT_HANDLED;
                continue;
            }
            if info.ExceptionAddress as u64 != address {
                *continue_kind = DBG_EXCEPTION_NOT_HANDLED;
                continue;
            }

            // println!("expection: {:?}", info.ExceptionAddress);
            // println!("originate: {:?}", address);

            w!(WriteProcessMemory(
                process,
                address as *mut _,
                &original as *const _ as *const _,
                1,
                &mut rw,
            ));

            let thread = OpenThread(THREAD_GET_CONTEXT | THREAD_SET_CONTEXT, 0, *tid);
            let mut regs = CONTEXT::default();
            regs.ContextFlags = CONTEXT_ALL;
            w!(GetThreadContext(thread, &mut regs));
            regs.Rip = address;
            w!(SetThreadContext(thread, &regs));
            *continue_kind = DBG_CONTINUE;

            return Ok(thread);
        }
    }
}

fn step(pid: u32, tid: &mut u32, continue_kind: &mut u32) -> Result<()> {
    unsafe {
        let thread = OpenThread(THREAD_GET_CONTEXT | THREAD_SET_CONTEXT, 0, *tid);
        let mut regs = CONTEXT::default();
        regs.ContextFlags = CONTEXT_ALL;
        if GetThreadContext(thread, &mut regs) == 0 {
            panic!();
        }
        regs.EFlags |= 0x100;
        if SetThreadContext(thread, &regs) == 0 {
            panic!();
        }
        CloseHandle(thread);

        loop {
            if ContinueDebugEvent(pid, *tid, *continue_kind) == 0 {
                panic!();
            }
            let dbg_event = wait_for_event();
            *tid = dbg_event.dwThreadId;
            if dbg_event.dwDebugEventCode != EXCEPTION_DEBUG_EVENT {
                if dbg_event.dwDebugEventCode == EXIT_PROCESS_DEBUG_EVENT {
                    panic!("ppt exited");
                }
                *continue_kind = DBG_EXCEPTION_NOT_HANDLED;
                continue;
            }

            let info = &dbg_event.u.Exception().ExceptionRecord;
            if info.ExceptionCode != EXCEPTION_SINGLE_STEP {
                *continue_kind = DBG_EXCEPTION_NOT_HANDLED;
                continue;
            }
            *continue_kind = DBG_CONTINUE;

            return Ok(());
        }
    }
}

fn find_ppt_process() -> Result<Option<u32>> {
    unsafe {
        let mut pids = [0; 4096];
        let mut used = 0;
        if EnumProcesses(
            pids.as_mut_ptr(),
            std::mem::size_of_val(&pids) as u32,
            &mut used,
        ) == 0
        {
            panic!("failed to enumerate processes");
        }

        for &process in &pids[..used as usize / std::mem::size_of::<u32>()] {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, process);
            if !handle.is_null() {
                let mut module = 0 as *mut _;
                if EnumProcessModules(
                    handle,
                    &mut module,
                    std::mem::size_of::<*mut ()>() as u32,
                    &mut used,
                ) != 0
                {
                    let mut buffer = vec![0; 4096];
                    GetModuleBaseNameW(
                        handle,
                        module,
                        buffer.as_mut_ptr(),
                        2 * buffer.len() as u32,
                    );
                    for i in 0..buffer.len() {
                        if buffer[i] == 0 {
                            let s = std::ffi::OsString::from_wide(&buffer[..i]);
                            if let Some(s) = s.to_str() {
                                if s == "puyopuyotetris.exe" {
                                    CloseHandle(handle);
                                    return Ok(Some(process));
                                }
                            }
                            break;
                        }
                    }
                }

                CloseHandle(handle);
            }
        }
        Ok(None)
    }
}

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
