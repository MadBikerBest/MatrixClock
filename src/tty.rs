//! Терминал без сторонних библиотек: raw-режим (termios), размер (TIOCGWINSZ),
//! ввод через poll/read с разбором клавиш. Обрыв терминала (закрыли окно,
//! SSH отвалился) — корректный выход, а не вечный цикл.

use crate::lock::Input;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static WINCH: AtomicBool = AtomicBool::new(false);

extern "C" fn on_winch(_: libc::c_int) {
    WINCH.store(true, Ordering::SeqCst);
}

/// Raw-режим на время жизни объекта.
pub struct Raw(libc::termios);

impl Raw {
    pub fn new() -> Result<Raw, String> {
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::isatty(0) == 0 || libc::tcgetattr(0, &mut t) != 0 {
                return Err("стандартный ввод — не терминал".into());
            }
            let orig = t;
            libc::cfmakeraw(&mut t);
            libc::tcsetattr(0, libc::TCSANOW, &t);
            libc::signal(libc::SIGWINCH, on_winch as *const () as libc::sighandler_t);
            Ok(Raw(orig))
        }
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &self.0) };
    }
}

/// Размер терминала (колонки, строки).
pub fn size() -> Option<(usize, usize)> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_col > 0 && ws.ws_row > 0;
    ok.then_some((ws.ws_col as usize, ws.ws_row as usize))
}

/// Был ли SIGWINCH с прошлого вызова.
pub fn resized() -> bool {
    WINCH.swap(false, Ordering::SeqCst)
}

/// Ждёт ввода не дольше timeout. Err — терминал закрыт.
pub fn wait_input(timeout: Duration, out: &mut Vec<Input>) -> Result<(), ()> {
    let mut pfd = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout.as_millis() as libc::c_int) };
    if rc <= 0 {
        return Ok(()); // таймаут или EINTR (например, SIGWINCH)
    }
    if pfd.revents & libc::POLLIN == 0 {
        return Err(()); // POLLHUP / POLLERR / POLLNVAL без данных
    }
    let mut buf = [0u8; 512];
    let n = unsafe { libc::read(0, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    if n == 0 {
        return Err(());
    }
    if n < 0 {
        let e = std::io::Error::last_os_error();
        return if e.kind() == std::io::ErrorKind::Interrupted { Ok(()) } else { Err(()) };
    }
    parse(&buf[..n as usize], out);
    Ok(())
}

/// Разбор байтов клавиатуры: нужны только символы, Enter, Backspace, Esc, Ctrl+U;
/// всё остальное (стрелки, F-клавиши, мышь) — «какое-то событие ввода».
fn parse(b: &[u8], out: &mut Vec<Input>) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        i += 1;
        out.push(match c {
            0x1b if i == b.len() => Input::Escape,
            0x1b => {
                match b[i] {
                    b'[' => {
                        i += 1;
                        if b.get(i) == Some(&b'M') {
                            i += 4; // мышь X10: ESC [ M + 3 байта
                        } else {
                            while i < b.len() && !(0x40..=0x7e).contains(&b[i]) {
                                i += 1;
                            }
                            i += 1;
                        }
                    }
                    b'O' => i += 2,
                    _ => i += 1, // Alt+клавиша
                }
                Input::Other
            }
            b'\r' | b'\n' => Input::Enter,
            0x7f | 0x08 => Input::Backspace,
            0x15 => Input::ClearLine,
            0..=0x1f => Input::Other,
            _ => {
                let len = match c {
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf7 => 4,
                    _ => 1,
                };
                let end = (i - 1 + len).min(b.len());
                let ch = std::str::from_utf8(&b[i - 1..end]).ok().and_then(|s| s.chars().next());
                i = end;
                ch.map_or(Input::Other, Input::Char)
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(b: &[u8]) -> Vec<String> {
        let mut v = Vec::new();
        parse(b, &mut v);
        v.iter()
            .map(|i| match i {
                Input::Char(c) => c.to_string(),
                Input::Enter => "ENTER".into(),
                Input::Backspace => "BS".into(),
                Input::Escape => "ESC".into(),
                Input::ClearLine => "CU".into(),
                Input::Other => "OTHER".into(),
            })
            .collect()
    }

    #[test]
    fn keys() {
        assert_eq!(p("aЖ\r".as_bytes()), ["a", "Ж", "ENTER"]);
        assert_eq!(p(b"\x7f\x15\x1b"), ["BS", "CU", "ESC"]);
        assert_eq!(p(b"\x1b[A\x1b[<35;10;5Mx"), ["OTHER", "OTHER", "x"]);
        assert_eq!(p(b"\x1bOPy"), ["OTHER", "y"]);
    }
}
