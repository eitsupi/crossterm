//! Windows VT-input diagnostic example.
//!
//! This example constructs crossterm's event reader before raw mode is enabled, then
//! enables raw mode and reads events with that same source. The mode is queried through
//! the same `CONIN$` handle type used by crossterm, so this exercises mode changes after
//! source construction and verifies that event parsing does not rely on a lifetime VT
//! cache. The diagnostic also prints the original and final VT-input bits.
//!
//! On Windows, run with:
//!
//! ```text
//! cargo run --example windows-vt-input --all-features
//! ```
//!
//! While it is running, press ordinary keys, F1, an Alt+numpad character, and paste
//! multiline text. Every event is printed with `Debug`, including `KeyEventKind` and
//! bracketed-paste events. Press Escape to finish.
//!
//! Expected observations on conhost include `a` and F1 producing both Press and Release
//! events (and Repeat while held), Alt+numpad 0233 producing a character event, and a
//! multiline paste producing one `Paste` event rather than one event per line.

#[cfg(windows)]
mod windows {
    use std::io;
    use std::time::Duration;

    use crossterm::event::{
        DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, poll, read,
    };
    use crossterm::execute;
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    use crossterm_winapi::{ConsoleMode, Handle};

    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;

    fn current_console_mode() -> io::Result<u32> {
        let handle = Handle::current_in_handle()?;
        ConsoleMode::from(handle).mode()
    }

    fn print_mode(label: &str, mode: u32) {
        let vt_input = mode & ENABLE_VIRTUAL_TERMINAL_INPUT != 0;
        println!("{label}: mode=0x{mode:08x}, VT_INPUT={vt_input}");
    }

    fn run_session() -> io::Result<()> {
        let mut stdout = io::stdout();
        let mut raw_enabled = false;

        let session_result = (|| {
            enable_raw_mode()?;
            raw_enabled = true;

            let raw_mode = current_console_mode()?;
            print_mode("after enable_raw_mode", raw_mode);
            if raw_mode & ENABLE_VIRTUAL_TERMINAL_INPUT != 0 {
                println!("raw-mode VT_INPUT activation: PASS");
            } else {
                println!(
                    "raw-mode VT_INPUT activation: legacy/unsupported fallback (not a failure)"
                );
            }
            execute!(stdout, EnableBracketedPaste)?;
            println!(
                "Reading events. Expected: a/F1 Press+Release (Repeat while held), Alt+numpad 0233 as a character, and multiline paste as one Paste event. Press Esc to exit."
            );

            loop {
                let event = read()?;
                println!("Event: {event:?}");

                if let Event::Key(key) = event
                    && key.code == KeyCode::Esc
                    && key.kind == KeyEventKind::Press
                {
                    break;
                }
            }

            Ok(())
        })();

        // Cleanup is deliberately unconditional: a command or read can fail after raw mode or
        // bracketed paste has already been enabled.
        let disable_paste_result = execute!(stdout, DisableBracketedPaste);
        let disable_raw_result = if raw_enabled {
            disable_raw_mode()
        } else {
            Ok(())
        };

        if let Err(error) = session_result {
            return Err(error);
        }
        disable_paste_result?;
        disable_raw_result?;
        Ok(())
    }

    pub(super) fn run() -> io::Result<()> {
        println!("Windows VT input diagnostic");
        let original_mode = current_console_mode()?;
        print_mode("before event::poll(Duration::ZERO)", original_mode);

        // This constructs the shared EVENT_READER and its Windows event source before raw mode.
        let poll_result = poll(Duration::ZERO)?;
        let after_poll_mode = current_console_mode()?;
        print_mode("after event::poll(Duration::ZERO)", after_poll_mode);
        println!(
            "event source construction mode preservation: {}",
            if original_mode == after_poll_mode {
                "PASS"
            } else {
                "FAIL"
            }
        );
        println!("initial poll result: {poll_result}");

        let session_result = run_session();

        let final_mode_result = current_console_mode();
        match final_mode_result {
            Ok(final_mode) => {
                print_mode("after cleanup", final_mode);
                let restored = final_mode & ENABLE_VIRTUAL_TERMINAL_INPUT
                    == original_mode & ENABLE_VIRTUAL_TERMINAL_INPUT;
                println!(
                    "original VT_INPUT bit restoration: {}",
                    if restored { "PASS" } else { "FAIL" }
                );
                if !restored && session_result.is_ok() {
                    return Err(io::Error::other(
                        "VT_INPUT bit was not restored after diagnostic cleanup",
                    ));
                }
            }
            Err(error) => {
                println!("after cleanup: unable to read console mode: {error}");
                println!("original VT_INPUT bit restoration: FAIL");
                if session_result.is_ok() {
                    return Err(error);
                }
            }
        }

        session_result
    }
}

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    windows::run()
}

#[cfg(not(windows))]
fn main() {
    println!("windows-vt-input is only available on Windows.");
}
