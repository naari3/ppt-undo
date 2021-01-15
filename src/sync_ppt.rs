use mpsc::{Receiver, Sender};
use std::os::windows::ffi::OsStringExt;
use std::sync::mpsc;
use winapi::um::debugapi::*;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::CloseHandle;
use winapi::um::memoryapi::{ReadProcessMemory, WriteProcessMemory};
use winapi::um::minwinbase::*;
use winapi::um::processthreadsapi::{GetThreadContext, OpenProcess, OpenThread, SetThreadContext};
use winapi::um::psapi::{EnumProcessModules, EnumProcesses, GetModuleBaseNameW};
use winapi::um::winbase::*;
use winapi::um::winnt::*;

use crate::Result;

use crate::state::{get_is_current_piece_active, get_seed, State};

pub enum Notify {
    Start(u16),
    Sync(State),
}

pub enum Message {
    Test,
}

// workaround for https://github.com/retep998/winapi-rs/issues/945
#[derive(Default)]
#[repr(align(16))]
struct Context(CONTEXT);

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

pub fn sync(notifier: Sender<Notify>, msger: Receiver<Message>) -> Result<()> {
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

        play(pid, &mut tid, &mut continue_kind, process, notifier, msger)?;

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
    msger: Receiver<Message>,
) -> Result<()> {
    // const INSTRUCTION_ADDRESS: u64 = 0x14025B8CC;
    const INPUT_SYSTEM_ADDRESS: u64 = 0x1413C7D9A;
    // const RNG_INITIAL_ADDRESS: u64 = 0x14003F87F;

    unsafe {
        let mut latest_seed = 0u16;
        let mut latest_state = State::new_blank();

        loop {
            let thread = breakpoint(pid, tid, continue_kind, process, INPUT_SYSTEM_ADDRESS)?;

            if let Ok(cmd) = msger.try_recv() {
                match cmd {
                    Message::Test => {
                        let mut regs = Context::default().0;
                        println!("%v {:p}", &regs);
                        regs.ContextFlags = CONTEXT_ALL;
                        w!(GetThreadContext(thread, &mut regs));
                        regs.Rbx = 0x40;
                        w!(SetThreadContext(thread, &regs));
                    }
                    _ => {}
                }
            }

            if let Ok(tmp_seed) = get_seed(process) {
                if tmp_seed != latest_seed {
                    notifier.send(Notify::Start(tmp_seed))?;
                    latest_seed = tmp_seed;
                    latest_state = State::new_blank();
                };
            };
            if let Ok(active) = get_is_current_piece_active(process) {
                if active {
                    let state = State::new_from_proc(process);
                    if let Ok(state) = state {
                        if state != latest_state && state.current_piece != None {
                            notifier.send(Notify::Sync(state.clone()))?;
                            latest_state = state;
                        }
                    }
                }
            }

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

            // println!("expection: {:?}", &info.ExceptionAddress);
            // println!("originate: {:x?}", address);

            w!(WriteProcessMemory(
                process,
                address as *mut _,
                &original as *const _ as *const _,
                1,
                &mut rw,
            ));

            let thread = OpenThread(THREAD_GET_CONTEXT | THREAD_SET_CONTEXT, 0, *tid);
            let mut regs = Context::default().0;
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
        let mut regs = Context::default().0;
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
