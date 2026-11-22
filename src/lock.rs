//! Логика блокировки: поле пароля, таймаут, попытки, сдвиг гаммы к красному.

use crate::config::Lock;
use crate::scene::Cell;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub enum Input {
    Char(char),
    Backspace,
    Enter,
    Escape,
    ClearLine,
    Other,
}

pub struct LockUi {
    cfg: Lock,
    user: String,
    visible: bool,
    buf: Zeroizing<String>,
    last_input: Instant,
    pub attempts: u32,
    checking: Option<Receiver<bool>>,
    denied_at: Option<Instant>,
    lockout_until: Option<Instant>,
    pub unlocked: bool,
}

impl LockUi {
    pub fn new(cfg: Lock, user: String) -> LockUi {
        LockUi {
            cfg,
            user,
            visible: false,
            buf: Zeroizing::new(String::new()),
            last_input: Instant::now(),
            attempts: 0,
            checking: None,
            denied_at: None,
            lockout_until: None,
            unlocked: false,
        }
    }

    /// Доля «красноты»: 0 — нет неверных попыток, 1 — исчерпаны все.
    pub fn tint(&self) -> f32 {
        (self.attempts as f32 / self.cfg.max_attempts as f32).min(1.0)
    }

    fn locked_out(&self) -> bool {
        self.lockout_until.is_some_and(|t| Instant::now() < t)
    }

    pub fn input(&mut self, ev: Input) {
        self.visible = true;
        self.last_input = Instant::now();
        if self.checking.is_some() || self.locked_out() {
            return;
        }
        match ev {
            Input::Char(c) if self.buf.len() < 512 => self.buf.push(c),
            Input::Backspace => {
                self.buf.pop();
            }
            Input::ClearLine => self.buf.clear(),
            Input::Escape => {
                self.buf.clear();
                self.visible = false;
            }
            Input::Enter if !self.buf.is_empty() => {
                let pw = Zeroizing::new(std::mem::take(&mut *self.buf));
                let (service, user) = (self.cfg.pam_service.clone(), self.user.clone());
                let (tx, rx) = channel();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::pam::authenticate(&service, &user, pw.as_bytes()));
                });
                self.checking = Some(rx);
            }
            _ => {}
        }
    }

    /// Вызывается каждый кадр: результат PAM, таймаут поля.
    pub fn tick(&mut self) {
        if let Some(ok) = self.checking.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.checking = None;
            self.last_input = Instant::now();
            if ok {
                self.unlocked = true;
            } else {
                self.attempts += 1;
                self.denied_at = Some(Instant::now());
                if self.attempts >= self.cfg.max_attempts {
                    self.lockout_until = Some(Instant::now() + Duration::from_secs(self.cfg.lockout_seconds as u64));
                }
            }
        }
        let idle = self.last_input.elapsed() > Duration::from_secs(self.cfg.input_timeout as u64);
        if self.visible && idle && self.checking.is_none() && !self.locked_out() {
            self.visible = false;
            self.buf.clear();
        }
    }

    /// Содержимое поля пароля: (строка ввода, строка состояния), если поле видно.
    /// room — сколько символов помещается в строку ввода.
    pub fn view(&self, room: usize, mask: &[char]) -> Option<(String, String)> {
        if !self.visible {
            return None;
        }
        let mut line = format!("{} ", self.cfg.prompt);
        let room = room.saturating_sub(line.chars().count() + 1);
        let n = self.buf.chars().count();
        let shown = n.min(room);
        line.extend((n - shown..n).map(|k| mask[(k * 7919 + 13) % mask.len()]));
        if self.checking.is_none() && !self.locked_out() && (now_ms() / 500).is_multiple_of(2) {
            line.push('_');
        }
        let status = if self.checking.is_some() {
            "VERIFYING...".to_string()
        } else if let Some(t) = self.lockout_until.filter(|t| Instant::now() < *t) {
            format!("LOCKED OUT  {}s", (t - Instant::now()).as_secs() + 1)
        } else if self.denied_at.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            format!("ACCESS DENIED  [{}/{}]", self.attempts, self.cfg.max_attempts)
        } else if self.attempts > 0 {
            format!("FAILED ATTEMPTS: {}/{}", self.attempts, self.cfg.max_attempts)
        } else {
            String::new()
        };
        Some((line, status))
    }

    /// Рисует поле пароля поверх текстового кадра.
    pub fn draw(&self, cells: &mut [Cell], cols: usize, rows: usize, below_row: usize, ascii: bool, mask: &[char]) {
        if cols < 12 || rows < 5 {
            return;
        }
        let w = 40.min(cols - 2);
        let Some((line, status)) = self.view(w - 4, mask) else { return };
        let x0 = (cols - w) / 2;
        let y0 = if below_row + 6 < rows { below_row + 2 } else { rows.saturating_sub(5) };
        let (h, v, tl, tr, bl, br) = if ascii { ('-', '|', '+', '+', '+', '+') } else { ('═', '║', '╔', '╗', '╚', '╝') };
        let mut put = |x: usize, y: usize, ch: char, fg: u8, bg: u8| {
            if x < cols && y < rows {
                cells[y * cols + x] = Cell { ch, fg, bg };
            }
        };
        let (frame, back) = (235u8, 18u8);
        for x in 0..w {
            let (a, b) = match x {
                0 => (tl, bl),
                _ if x == w - 1 => (tr, br),
                _ => (h, h),
            };
            put(x0 + x, y0, a, frame, back);
            put(x0 + x, y0 + 2, b, frame, back);
        }
        put(x0, y0 + 1, v, frame, back);
        put(x0 + w - 1, y0 + 1, v, frame, back);
        let line: Vec<char> = line.chars().collect();
        for x in 1..w - 1 {
            let ch = if x >= 2 { line.get(x - 2).copied().unwrap_or(' ') } else { ' ' };
            put(x0 + x, y0 + 1, ch, 255, back);
        }
        let sx = x0 + (w.saturating_sub(status.chars().count())) / 2;
        for (k, ch) in status.chars().enumerate() {
            put(sx + k, y0 + 3, ch, 220, back);
        }
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())
}
