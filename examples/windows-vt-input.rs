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
    use std::collections::HashMap;
    use std::io;
    use std::time::Duration;

    use crossterm::event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind, poll, read,
    };
    use crossterm::execute;
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    use crossterm_winapi::{ConsoleMode, Handle};

    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
    const ENABLE_MOUSE_INPUT: u32 = 0x0010;
    const ENABLE_WINDOW_INPUT: u32 = 0x0008;

    #[derive(Default)]
    struct SessionStats {
        mouse_moved: u64,
        mouse_clicks: u64,
        mouse_drags: u64,
        mouse_wheels: u64,
        key_presses: u64,
        key_releases: u64,
        press_balance: HashMap<(KeyCode, KeyModifiers), u32>,
        duplicate_candidates: u64,
        last_alt_press: Option<KeyCode>,
        duplicate_candidate_key: Option<KeyCode>,
    }

    impl SessionStats {
        fn record(&mut self, event: &Event) {
            match event {
                Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Moved) => {
                    self.mouse_moved += 1;
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(_) | MouseEventKind::Up(_) => self.mouse_clicks += 1,
                    MouseEventKind::Drag(_) => self.mouse_drags += 1,
                    MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight => self.mouse_wheels += 1,
                    MouseEventKind::Moved => unreachable!(),
                },
                Event::Key(key) if key.code == KeyCode::Esc => {
                    // Escape terminates the diagnostic; do not report its intentional lone Press
                    // as an unmatched key in the final summary.
                }
                Event::Key(key) => match key.kind {
                    KeyEventKind::Press => {
                        self.key_presses += 1;
                        let identity = (key.code, key.modifiers);
                        if self.duplicate_candidate_key == Some(key.code) {
                            self.duplicate_candidates += 1;
                            self.duplicate_candidate_key = None;
                        }
                        if key.modifiers.contains(KeyModifiers::ALT) {
                            self.last_alt_press = Some(key.code);
                        } else {
                            self.last_alt_press = None;
                        }
                        *self.press_balance.entry(identity).or_default() += 1;
                    }
                    KeyEventKind::Release => {
                        self.key_releases += 1;
                        let identity = (key.code, key.modifiers);
                        if let Some(balance) = self.press_balance.get_mut(&identity) {
                            *balance = balance.saturating_sub(1);
                        }
                        self.duplicate_candidate_key = self
                            .last_alt_press
                            .filter(|press_code| *press_code == key.code);
                        self.last_alt_press = None;
                    }
                    KeyEventKind::Repeat => {}
                },
                _ => {}
            }
        }

        fn print_summary(&self) {
            let mismatches = self
                .press_balance
                .values()
                .filter(|count| **count != 0)
                .count();
            println!("\nSummary:");
            println!(
                "  key Press={}, Release={}",
                self.key_presses, self.key_releases
            );
            println!(
                "  mouse Moved={} (suppressed from log), click={}, drag={}, wheel={}",
                self.mouse_moved, self.mouse_clicks, self.mouse_drags, self.mouse_wheels
            );
            println!("  Press/Release mismatches={mismatches}");
            println!(
                "  ALT Press -> Release -> same-code NONE Press candidates={} (manual check)",
                self.duplicate_candidates
            );
        }
    }

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
        let mut mouse_enabled = false;
        let mut paste_enabled = false;
        let mut stats = SessionStats::default();

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
            println!(
                "mouse mode bits after raw mode: MOUSE_INPUT={}, WINDOW_INPUT={}",
                raw_mode & ENABLE_MOUSE_INPUT != 0,
                raw_mode & ENABLE_WINDOW_INPUT != 0
            );
            execute!(stdout, EnableMouseCapture)?;
            mouse_enabled = true;
            let mouse_mode = current_console_mode()?;
            print_mode("after EnableMouseCapture", mouse_mode);
            execute!(stdout, EnableBracketedPaste)?;
            paste_enabled = true;
            println!(
                "Reading events. Try click, drag, wheel, paste, ordinary keys, Ctrl+Shift+letter, and Alt+numpad 0233. Moved events are counted but not logged. Press Esc to exit."
            );

            loop {
                let event = read()?;
                stats.record(&event);
                if !matches!(&event, Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Moved))
                {
                    println!("Event: {event:?}");
                }

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
        let mut cleanup_error = None;
        if mouse_enabled {
            if let Err(error) = execute!(stdout, DisableMouseCapture) {
                cleanup_error = Some(error);
            }
        }
        if paste_enabled {
            if let Err(error) = execute!(stdout, DisableBracketedPaste) {
                cleanup_error.get_or_insert(error);
            }
        }
        let disable_raw_result = if raw_enabled {
            disable_raw_mode()
        } else {
            Ok(())
        };
        if let Err(error) = disable_raw_result {
            cleanup_error.get_or_insert(error);
        }
        stats.print_summary();

        if let Err(error) = session_result {
            return Err(error);
        }
        if let Some(error) = cleanup_error {
            return Err(error);
        }
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
                let restore_mask =
                    ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_MOUSE_INPUT | ENABLE_WINDOW_INPUT;
                let restored = final_mode & restore_mask == original_mode & restore_mask;
                println!(
                    "original VT_INPUT/MOUSE_INPUT/WINDOW_INPUT restoration: {}",
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
