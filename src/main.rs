mod config;
mod daemon;
mod gfx;
mod info;
mod lock;
mod pam;
mod scene;
mod shape;
mod term;
mod tty;
mod xwin;

use config::{Config, Mode};
use lock::{Input, LockUi};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const HELP: &str = "\
matrixclock — консольный скринсейвер-часы в стиле Matrix с блокировкой

  matrixclock [ключи]

  --config <файл>   файл настроек (по умолчанию ~/.config/matrixclock/config.toml,
                    затем /etc/matrixclock/config.toml; если нет обоих —
                    ~/.config/matrixclock/config.toml создаётся из примера)
  --text            текстовый режим даже при наличии X ($DISPLAY)
  --full | --lite   текстовый режим: принудительно 24 бит / 16 цветов
  --monitor <N>     графика только на мониторе N (0, 1, …), по умолчанию — на всех
  --no-lock         не спрашивать пароль, выход по любой клавише
  --print-config    вывести пример конфига со всеми параметрами
  --idle-seconds    вывести время простоя из конфига в секундах (для tmux)
  -h, --help        эта справка

Фоновые режимы (запуск после general.idle_minutes без ввода):
  --daemon          в сеансе X11: следит за простоем, запускает скринсейвер.
                    Остальные ключи (--config, --no-lock, --monitor) передаются ему.
  --console-daemon  служба для текстовых консолей (root, ставится --install-system)

Регистрация без пакетов (путь к программе — тот, откуда она запущена):
  --install           автозапуск --daemon при входе в X (~/.config/autostart),
                      другие ключи этой команды попадут в автозапуск
  --uninstall         убрать автозапуск
  sudo matrixclock --install-system
                      копия в /usr/local/bin, /etc/pam.d/matrixclock,
                      служба matrixclock-console.service (включается сразу)
  sudo matrixclock --uninstall-system
                      убрать службу и копию (PAM-файл остаётся)
  --user <имя>        чей пароль спрашивать (только root; использует служба)
";

pub(crate) static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

pub(crate) const VT_LOCKSWITCH: libc::c_ulong = 0x560B;
pub(crate) const VT_UNLOCKSWITCH: libc::c_ulong = 0x560C;

/// Запрет переключения VT на время блокировки (как vlock -a).
struct VtLock;
impl VtLock {
    fn new() -> Result<VtLock, String> {
        if unsafe { libc::ioctl(0, VT_LOCKSWITCH, 0) } != 0 {
            return Err(format!(
                "lock_all_vt: не удалось запретить переключение VT ({}).\n\
                 Нужен root или: sudo setcap cap_sys_tty_config+ep $(command -v matrixclock)",
                std::io::Error::last_os_error()
            ));
        }
        Ok(VtLock)
    }
}
impl Drop for VtLock {
    fn drop(&mut self) {
        unsafe { libc::ioctl(0, VT_UNLOCKSWITCH, 0) };
    }
}

/// Шрифт консоли на время работы (штатный setfont, прежний сохраняется через -O).
struct ConsoleFont(std::path::PathBuf);
impl ConsoleFont {
    fn set(name: &str) -> Option<ConsoleFont> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let saved = dir.join(format!("matrixclock-font-{}.psf", std::process::id()));
        let ok = std::process::Command::new("setfont")
            .arg("-O")
            .arg(&saved)
            .arg(name)
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        ok.then_some(ConsoleFont(saved))
    }
}
impl Drop for ConsoleFont {
    fn drop(&mut self) {
        let _ = std::process::Command::new("setfont").arg(&self.0).status();
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Текущее локальное время: (часы, минуты, секунды).
fn local_time() -> (u32, u32, u32) {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        (tm.tm_hour as u32, tm.tm_min as u32, tm.tm_sec as u32)
    }
}

fn clock_text(cfg: &Config) -> (String, bool) {
    let (mut h, m, s) = local_time();
    if cfg.clock.hours == 12 {
        h = if h % 12 == 0 { 12 } else { h % 12 };
    }
    let text = if cfg.clock.seconds { format!("{h:02}:{m:02}:{s:02}") } else { format!("{h:02}:{m:02}") };
    (text, !cfg.clock.blink_colon || s % 2 == 0)
}

#[derive(PartialEq)]
enum Action {
    Run,
    Daemon,
    ConsoleDaemon,
    Install,
    Uninstall,
    InstallSystem,
    UninstallSystem,
}

struct Args {
    action: Action,
    user: Option<String>,
    config: Option<std::path::PathBuf>,
    mode: Option<Mode>,
    no_lock: bool,
    text: bool,
    monitor: Option<usize>,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut a = Args { action: Action::Run, user: None, config: None, mode: None, no_lock: false, text: false, monitor: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => a.config = Some(it.next().ok_or("--config: нужен путь")?.into()),
            "--full" => a.mode = Some(Mode::Full),
            "--lite" => a.mode = Some(Mode::Lite),
            "--no-lock" => a.no_lock = true,
            "--daemon" => a.action = Action::Daemon,
            "--console-daemon" => a.action = Action::ConsoleDaemon,
            "--install" => a.action = Action::Install,
            "--uninstall" => a.action = Action::Uninstall,
            "--install-system" => a.action = Action::InstallSystem,
            "--uninstall-system" => a.action = Action::UninstallSystem,
            "--user" => a.user = Some(it.next().ok_or("--user: нужно имя")?),
            "--text" => a.text = true,
            "--monitor" => a.monitor = Some(it.next().and_then(|n| n.parse().ok()).ok_or("--monitor: нужен номер")?),
            "--print-config" => {
                print!("{}", config::EXAMPLE);
                return Ok(None);
            }
            "--idle-seconds" => {
                println!("{}", Config::load(a.config.clone())?.general.idle_minutes * 60);
                return Ok(None);
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            other => return Err(format!("неизвестный ключ {other}, см. --help")),
        }
    }
    Ok(Some(a))
}

fn run() -> Result<(), String> {
    let Some(args) = parse_args()? else { return Ok(()) };
    let mut cfg = Config::load(args.config)?;
    if args.no_lock {
        cfg.lock.enabled = false;
    }
    match args.action {
        Action::Install => return daemon::install(&cfg),
        Action::Uninstall => return daemon::uninstall(),
        Action::InstallSystem => return daemon::install_system(),
        Action::UninstallSystem => return daemon::uninstall_system(),
        _ => {}
    }
    let uid = unsafe { libc::getuid() };
    let user = match args.user {
        Some(_) if uid != 0 => return Err("--user — только для root (его использует служба консоли)".into()),
        Some(u) => u,
        None => pam::user_name(uid).ok_or("не удалось определить имя пользователя")?,
    };
    if cfg.lock.enabled && !pam::service_exists(&cfg.lock.pam_service) {
        return Err(format!(
            "нет сервиса PAM \"{0}\" (/etc/pam.d/{0}) — разблокировать будет невозможно.\n\
             Установите: sudo {1} --install-system   (PAM-файл и служба консоли)\n\
             или только PAM-файл (из каталога проекта):\n\
             sudo install -m644 pam/matrixclock /etc/pam.d/matrixclock\n\
             или запустите с --no-lock",
            cfg.lock.pam_service,
            std::env::current_exe().unwrap_or_default().display()
        ));
    }
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, on_signal as *const () as libc::sighandler_t);
    }
    match args.action {
        Action::Daemon => return daemon::run_x(&cfg),
        Action::ConsoleDaemon => return daemon::run_console(&cfg),
        _ => {}
    }
    if std::env::var_os("DISPLAY").is_some() && !args.text {
        return run_x(&cfg, user, args.monitor);
    }
    run_text(&cfg, user, args.mode)
}

/// Графический режим (X11).
fn run_x(cfg: &Config, user: String, monitor: Option<usize>) -> Result<(), String> {
    let (mut xw, mons) = xwin::XWin::open(monitor)?;
    xw.grab()?;
    let gl = unsafe { glow::Context::from_loader_function(|s| xw.proc_address(s)) };
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
    let mut gfx = gfx::Gfx::new(gl, cfg, xw.w, xw.h, mons, seed)?;
    let pal = term::Palette::new(&cfg.colors)?;
    let mask = scene::charset(&cfg.rain.charset, false);
    let mut lock = LockUi::new(cfg.lock.clone(), user);
    let frame = Duration::from_secs_f32(1.0 / cfg.graphics.fps as f32);
    let lang = info::resolve(cfg.general.language);
    let weather = info::start(cfg.weather.location.clone(), cfg.path.clone(), lang);
    let start = Instant::now();
    let mut last = start;
    let mut inputs = Vec::new();
    while !STOP.load(Ordering::SeqCst) {
        let deadline = last + frame;
        // ввод читается в каждом кадре, даже если отрисовка медленнее fps
        loop {
            xw.events(deadline.saturating_duration_since(Instant::now()), &mut inputs);
            if Instant::now() >= deadline {
                break;
            }
        }
        for input in inputs.drain(..) {
            // первые полсекунды игнорируем «шевеление» мыши от появления окна
            if matches!(input, Input::Other) && start.elapsed() < Duration::from_millis(500) {
                continue;
            }
            if !cfg.lock.enabled {
                return Ok(());
            }
            lock.input(input);
        }
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        lock.tick();
        if lock.unlocked {
            return Ok(());
        }
        let (text, colon) = clock_text(cfg);
        let w = weather.lock().ok().and_then(|w| w.clone());
        let info = (w.as_ref(), info::date_text(lang));
        gfx.frame(dt, &text, colon, lock.view(34, &mask), (info.0, &info.1), term::tinted(pal, lock.tint()));
        xw.swap();
    }
    Ok(())
}

/// Текстовый режим: X-терминал, SSH, голая консоль.
fn run_text(cfg: &Config, user: String, mode: Option<Mode>) -> Result<(), String> {
    let cfg = cfg.clone();
    let linux_console = std::env::var("TERM").is_ok_and(|t| t == "linux");
    let lite = match mode.unwrap_or(cfg.general.mode) {
        Mode::Auto => linux_console,
        m => m == Mode::Lite,
    };
    let _vt = if cfg.lock.enabled && cfg.lock.lock_all_vt && linux_console { Some(VtLock::new()?) } else { None };

    let fps = cfg.console.fps;
    let frame = Duration::from_secs_f32(1.0 / fps as f32);
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);

    let _font = if lite && linux_console && !cfg.console.font.is_empty() { ConsoleFont::set(&cfg.console.font) } else { None };
    let _raw = tty::Raw::new()?;
    let (c, r) = tty::size().ok_or("не удалось узнать размер терминала")?;
    let mut scene = scene::Scene::new(&cfg, c, r, lite, seed);
    let mask = scene::charset(&cfg.rain.charset, lite);
    let mut lock = LockUi::new(cfg.lock.clone(), user);
    let mut term = term::Term::new(lite, linux_console, term::Palette::new(&cfg.colors)?);
    let mut cells = Vec::new();
    let lang = info::resolve(cfg.general.language);
    let weather = info::start(cfg.weather.location.clone(), cfg.path.clone(), lang);
    let mut tint = 0.0;
    let mut last = Instant::now();

    while !STOP.load(Ordering::SeqCst) {
        let deadline = last + frame;
        // ввод читается в каждом кадре, даже если отрисовка медленнее fps
        loop {
            let mut inputs = Vec::new();
            if tty::wait_input(deadline.saturating_duration_since(Instant::now()), &mut inputs).is_err() {
                return Ok(()); // терминал закрыт
            }
            if tty::resized()
                && let Some((c, r)) = tty::size()
            {
                scene = scene::Scene::new(&cfg, c, r, lite, seed ^ last.elapsed().as_nanos() as u64);
            }
            for input in inputs {
                if !cfg.lock.enabled {
                    return Ok(());
                }
                lock.input(input);
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.2);
        last = now;

        lock.tick();
        if lock.unlocked {
            return Ok(());
        }
        if lock.tint() != tint {
            tint = lock.tint();
            term.set_tint(tint);
        }
        let (text, colon) = clock_text(&cfg);
        scene.set_text(&text, colon);
        scene.update(dt);
        scene.compose(&mut cells);
        let w = weather.lock().ok().and_then(|w| w.clone());
        // поле пароля встаёт на место даты
        let date = if lock.view(0, &mask).is_some() { String::new() } else { info::date_text(lang) };
        let date_row = scene.overlay(&mut cells, w.as_ref(), &date);
        lock.draw(&mut cells, scene.cols, scene.rows, date_row.saturating_sub(3), lite, &mask);
        term.draw(&cells, scene.cols).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("matrixclock: {e}");
        std::process::exit(1);
    }
}
