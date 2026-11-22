//! Окно X11 поверх всех мониторов: GLX-контекст, захват клавиатуры/мыши,
//! ввод через XIM (пароль набирается в любой раскладке), мониторы через XRandR.

use crate::lock::Input;
use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ptr::{null, null_mut};
use std::time::Duration;
use x11::{glx, keysym, xlib, xrandr, xss};

#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub struct XWin {
    dpy: *mut xlib::Display,
    win: xlib::Window,
    ctx: glx::GLXContext,
    im: xlib::XIM,
    ic: xlib::XIC,
    cursor: xlib::Cursor,
    pub w: i32,
    pub h: i32,
}

fn monitors(dpy: *mut xlib::Display, root: xlib::Window) -> Vec<Monitor> {
    let mut n: c_int = 0;
    let mut out = Vec::new();
    unsafe {
        let m = xrandr::XRRGetMonitors(dpy, root, xlib::True, &mut n);
        if !m.is_null() {
            for i in 0..n as usize {
                let mi = &*m.add(i);
                out.push(Monitor { x: mi.x, y: mi.y, w: mi.width, h: mi.height });
            }
            xrandr::XRRFreeMonitors(m);
        }
    }
    out
}

impl XWin {
    /// Открывает окно на весь экран X (или на один монитор, если указан номер).
    /// Возвращает окно и мониторы в координатах окна.
    pub fn open(only: Option<usize>) -> Result<(XWin, Vec<Monitor>), String> {
        unsafe {
            libc::setlocale(libc::LC_CTYPE, c"".as_ptr());
            xlib::XSetLocaleModifiers(c"".as_ptr());
            let dpy = xlib::XOpenDisplay(null());
            if dpy.is_null() {
                return Err("не удалось подключиться к X-серверу ($DISPLAY)".into());
            }
            let screen = xlib::XDefaultScreen(dpy);
            let root = xlib::XRootWindow(dpy, screen);
            let full = Monitor { x: 0, y: 0, w: xlib::XDisplayWidth(dpy, screen), h: xlib::XDisplayHeight(dpy, screen) };
            let mut mons = monitors(dpy, root);
            if mons.is_empty() {
                mons.push(full);
            }
            let area = match only {
                Some(i) => *mons.get(i).ok_or_else(|| format!("нет монитора {i}, всего {}", mons.len()))?,
                None => full,
            };
            let mons: Vec<Monitor> = mons
                .into_iter()
                .filter(|m| only.is_none() || (m.x == area.x && m.y == area.y))
                .map(|m| Monitor { x: m.x - area.x, y: m.y - area.y, ..m })
                .collect();

            let mut attrs = [
                glx::GLX_RGBA,
                glx::GLX_DOUBLEBUFFER,
                glx::GLX_RED_SIZE,
                8,
                glx::GLX_GREEN_SIZE,
                8,
                glx::GLX_BLUE_SIZE,
                8,
                0,
            ];
            let vi = glx::glXChooseVisual(dpy, screen, attrs.as_mut_ptr());
            if vi.is_null() {
                xlib::XCloseDisplay(dpy);
                return Err("нет подходящего OpenGL-визуала (GLX)".into());
            }
            // невидимый курсор
            let empty = [0 as c_char; 8];
            let pm = xlib::XCreateBitmapFromData(dpy, root, empty.as_ptr(), 1, 1);
            let mut black: xlib::XColor = std::mem::zeroed();
            let cursor = xlib::XCreatePixmapCursor(dpy, pm, pm, &mut black, &mut black, 0, 0);
            xlib::XFreePixmap(dpy, pm);

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.colormap = xlib::XCreateColormap(dpy, root, (*vi).visual, xlib::AllocNone);
            swa.event_mask = xlib::KeyPressMask
                | xlib::ButtonPressMask
                | xlib::PointerMotionMask
                | xlib::ExposureMask
                | xlib::VisibilityChangeMask;
            swa.override_redirect = xlib::True;
            swa.cursor = cursor;
            let win = xlib::XCreateWindow(
                dpy,
                root,
                area.x,
                area.y,
                area.w as c_uint,
                area.h as c_uint,
                0,
                (*vi).depth,
                xlib::InputOutput as c_uint,
                (*vi).visual,
                xlib::CWColormap | xlib::CWEventMask | xlib::CWOverrideRedirect | xlib::CWBackPixel | xlib::CWBorderPixel | xlib::CWCursor,
                &mut swa,
            );
            xlib::XStoreName(dpy, win, c"matrixclock".as_ptr());
            xlib::XMapRaised(dpy, win);
            xlib::XSync(dpy, xlib::False);

            let ctx = glx::glXCreateContext(dpy, vi, null_mut(), xlib::True);
            xlib::XFree(vi as *mut c_void);
            if ctx.is_null() || glx::glXMakeCurrent(dpy, win, ctx) == 0 {
                xlib::XDestroyWindow(dpy, win);
                xlib::XCloseDisplay(dpy);
                return Err("не удалось создать OpenGL-контекст".into());
            }

            let im = xlib::XOpenIM(dpy, null_mut(), null_mut(), null_mut());
            let ic = if im.is_null() {
                null_mut()
            } else {
                xlib::XCreateIC(
                    im,
                    xlib::XNInputStyle_0.as_ptr() as *const c_char,
                    (xlib::XIMPreeditNothing | xlib::XIMStatusNothing) as c_ulong,
                    xlib::XNClientWindow_0.as_ptr() as *const c_char,
                    win,
                    null::<c_void>(),
                )
            };
            Ok((XWin { dpy, win, ctx, im, ic, cursor, w: area.w, h: area.h }, mons))
        }
    }

    /// Захват клавиатуры и мыши: без него override-redirect окно не получает
    /// нажатий, а при блокировке ввод не должен уходить другим окнам.
    pub fn grab(&self) -> Result<(), String> {
        for _ in 0..100 {
            let ok = unsafe {
                let k = xlib::XGrabKeyboard(self.dpy, self.win, xlib::True, xlib::GrabModeAsync, xlib::GrabModeAsync, xlib::CurrentTime);
                let p = xlib::XGrabPointer(
                    self.dpy,
                    self.win,
                    xlib::True,
                    (xlib::ButtonPressMask | xlib::PointerMotionMask) as c_uint,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                    self.win,
                    self.cursor,
                    xlib::CurrentTime,
                );
                k == xlib::GrabSuccess && p == xlib::GrabSuccess
            };
            if ok {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("не удалось захватить клавиатуру/мышь (их держит другая программа)".into())
    }

    pub fn proc_address(&self, name: &str) -> *const c_void {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { glx::glXGetProcAddress(c.as_ptr() as *const u8).map_or(null(), |f| f as *const c_void) }
    }

    pub fn swap(&self) {
        unsafe { glx::glXSwapBuffers(self.dpy, self.win) };
    }

    /// Ждёт события не дольше timeout и складывает ввод в out.
    pub fn events(&mut self, timeout: Duration, out: &mut Vec<Input>) {
        unsafe {
            if xlib::XPending(self.dpy) == 0 {
                let mut pfd = libc::pollfd { fd: xlib::XConnectionNumber(self.dpy), events: libc::POLLIN, revents: 0 };
                libc::poll(&mut pfd, 1, timeout.as_millis() as c_int);
            }
            while xlib::XPending(self.dpy) > 0 {
                let mut ev: xlib::XEvent = std::mem::zeroed();
                xlib::XNextEvent(self.dpy, &mut ev);
                if xlib::XFilterEvent(&mut ev, 0) != 0 {
                    continue;
                }
                match ev.get_type() {
                    xlib::KeyPress => self.key(&mut ev.key, out),
                    xlib::ButtonPress | xlib::MotionNotify => out.push(Input::Other),
                    xlib::VisibilityNotify => {
                        xlib::XRaiseWindow(self.dpy, self.win);
                    }
                    _ => {}
                }
            }
        }
    }

    unsafe fn key(&self, ev: &mut xlib::XKeyEvent, out: &mut Vec<Input>) {
        let mut buf = [0 as c_char; 64];
        let mut sym: c_ulong = 0;
        let mut status: c_int = 0;
        let n = unsafe {
            if self.ic.is_null() {
                xlib::XLookupString(ev, buf.as_mut_ptr(), buf.len() as c_int, &mut sym, null_mut())
            } else {
                xlib::Xutf8LookupString(self.ic, ev, buf.as_mut_ptr(), buf.len() as c_int, &mut sym, &mut status)
            }
        };
        let ctrl = ev.state & xlib::ControlMask != 0;
        let s = sym as c_uint;
        out.push(match s {
            keysym::XK_Return | keysym::XK_KP_Enter => Input::Enter,
            keysym::XK_BackSpace => Input::Backspace,
            keysym::XK_Escape => Input::Escape,
            keysym::XK_u | keysym::XK_U if ctrl => Input::ClearLine,
            _ if ctrl => Input::Other,
            _ => {
                let bytes: Vec<u8> = buf[..n.max(0) as usize].iter().map(|&b| b as u8).collect();
                let text = String::from_utf8_lossy(&bytes);
                let chars: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
                if chars.is_empty() {
                    Input::Other
                } else {
                    for c in chars {
                        out.push(Input::Char(c));
                    }
                    return;
                }
            }
        });
    }
}

/// Время простоя X (с последнего ввода) — штатное расширение MIT-SCREEN-SAVER.
pub struct Idle {
    dpy: *mut xlib::Display,
    info: *mut xss::XScreenSaverInfo,
}

impl Idle {
    pub fn open() -> Result<Idle, String> {
        unsafe {
            let dpy = xlib::XOpenDisplay(null());
            if dpy.is_null() {
                return Err("не удалось подключиться к X-серверу ($DISPLAY)".into());
            }
            let (mut ev, mut err) = (0, 0);
            if xss::XScreenSaverQueryExtension(dpy, &mut ev, &mut err) == 0 {
                xlib::XCloseDisplay(dpy);
                return Err("X-сервер без расширения MIT-SCREEN-SAVER — простой не узнать".into());
            }
            Ok(Idle { dpy, info: xss::XScreenSaverAllocInfo() })
        }
    }

    pub fn get(&self) -> Duration {
        unsafe {
            xss::XScreenSaverQueryInfo(self.dpy, xlib::XDefaultRootWindow(self.dpy), self.info);
            Duration::from_millis((*self.info).idle)
        }
    }
}

impl Drop for Idle {
    fn drop(&mut self) {
        unsafe {
            xlib::XFree(self.info as *mut c_void);
            xlib::XCloseDisplay(self.dpy);
        }
    }
}

impl Drop for XWin {
    fn drop(&mut self) {
        unsafe {
            xlib::XUngrabKeyboard(self.dpy, xlib::CurrentTime);
            xlib::XUngrabPointer(self.dpy, xlib::CurrentTime);
            glx::glXMakeCurrent(self.dpy, 0, null_mut());
            glx::glXDestroyContext(self.dpy, self.ctx);
            if !self.ic.is_null() {
                xlib::XDestroyIC(self.ic);
            }
            if !self.im.is_null() {
                xlib::XCloseIM(self.im);
            }
            xlib::XDestroyWindow(self.dpy, self.win);
            xlib::XFreeCursor(self.dpy, self.cursor);
            xlib::XCloseDisplay(self.dpy);
        }
    }
}
