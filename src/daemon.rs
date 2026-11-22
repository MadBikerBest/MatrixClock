//! Фоновые режимы и их регистрация в системе (без пакетов).
//!   --daemon          — в сеансе X: следит за простоем (расширение MIT-SCREEN-SAVER)
//!                       и запускает скринсейвер отдельным процессом;
//!   --console-daemon  — служба systemd (root) для текстовых консолей: простой —
//!                       время последнего ввода на терминале (как у `w`), блокировка
//!                       на отдельной свободной консоли с запретом переключения
//!                       (как physlock / vlock -an);
//!   --install / --uninstall               — автозапуск X для пользователя (XDG autostart);
//!   --install-system / --uninstall-system — служба консоли, PAM-файл (root).

use crate::config::Config;
use crate::{STOP, VT_LOCKSWITCH, VT_UNLOCKSWITCH, pam, xwin};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

// linux/vt.h, linux/kd.h
const VT_OPENQRY: libc::c_ulong = 0x5600;
const VT_GETSTATE: libc::c_ulong = 0x5603;
const VT_ACTIVATE: libc::c_ulong = 0x5606;
const VT_WAITACTIVE: libc::c_ulong = 0x5607;
const VT_DISALLOCATE: libc::c_ulong = 0x5608;
const KDGETMODE: libc::c_ulong = 0x4B3B;
const KD_TEXT: libc::c_int = 0;

const BIN: &str = "/usr/local/bin/matrixclock";
const PKG_BIN: &str = "/usr/bin/matrixclock"; // установлена пакетом
const SERVICE: &str = "matrixclock-console.service";
const UNIT: &str = "/etc/systemd/system/matrixclock-console.service";
const PAM_FILE: &str = "/etc/pam.d/matrixclock";
const PAM_TEXT: &str = include_str!("../pam/matrixclock");
const UNIT_TEXT: &str = "\
# Создано командой: matrixclock --install-system
# Удаление:         sudo matrixclock --uninstall-system
# Настройки службы: /etc/matrixclock/config.toml
[Unit]
Description=MatrixClock screensaver and lock for text consoles
After=systemd-user-sessions.service

[Service]
ExecStart=@BIN@ --console-daemon
Restart=on-failure

[Install]
WantedBy=multi-user.target
";

fn stopped() -> bool {
    STOP.load(Ordering::SeqCst)
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Аргументы командной строки без указанного ключа — для запуска скринсейвера
/// с теми же настройками (--config, --no-lock, --monitor …).
pub fn args_without(flag: &str) -> Vec<String> {
    std::env::args().skip(1).filter(|a| a != flag).collect()
}

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("не удалось узнать путь к программе: {e}"))
}

fn io_err(path: impl AsRef<Path>) -> impl FnOnce(std::io::Error) -> String {
    move |e| format!("{}: {e}", path.as_ref().display())
}

// ─────────────────────────────── сеанс X ───────────────────────────────

/// Один фоновый процесс на дисплей.
fn single_instance() -> Result<File, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or("не задан XDG_RUNTIME_DIR")?;
    let display = std::env::var("DISPLAY").unwrap_or_default().replace('/', "_");
    let path = PathBuf::from(dir).join(format!("matrixclock-daemon{display}.lock"));
    let f = File::create(&path).map_err(io_err(&path))?;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!("фоновый режим для DISPLAY={display} уже запущен"));
    }
    Ok(f)
}

pub fn run_x(cfg: &Config) -> Result<(), String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return Err("--daemon работает только в сеансе X11 (сейчас Wayland)".into());
    }
    let _instance = single_instance()?;
    let idle = xwin::Idle::open()?;
    let limit = Duration::from_secs(cfg.general.idle_minutes as u64 * 60);
    let (exe, args) = (exe()?, args_without("--daemon"));
    let mut child: Option<std::process::Child> = None;
    let mut armed = true; // после разблокировки — false до первого ввода
    let mut retry = Instant::now();
    while !stopped() {
        std::thread::sleep(Duration::from_secs(1));
        if let Some(c) = &mut child {
            match c.try_wait() {
                Ok(None) => continue,
                Ok(Some(st)) if st.success() => armed = false,
                Ok(Some(st)) => {
                    eprintln!("matrixclock: скринсейвер завершился с ошибкой ({st}), повтор через 10 с");
                    retry = Instant::now() + Duration::from_secs(10);
                }
                Err(e) => return Err(format!("ожидание скринсейвера: {e}")),
            }
            child = None;
        }
        let t = idle.get();
        if t < limit {
            armed = true;
        }
        if armed && t >= limit && Instant::now() >= retry {
            child = Some(Command::new(&exe).args(&args).spawn().map_err(io_err(&exe))?);
        }
    }
    Ok(())
}

// ─────────────────────────── текстовые консоли ───────────────────────────

pub fn run_console(cfg: &Config) -> Result<(), String> {
    if !is_root() {
        return Err("--console-daemon запускает служба systemd от root (см. --install-system)".into());
    }
    let tty0 = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open("/dev/tty0")
        .map_err(io_err("/dev/tty0"))?;
    let limit = Duration::from_secs(cfg.general.idle_minutes as u64 * 60);
    let (exe, args) = (exe()?, args_without("--console-daemon"));
    let mut unlocked: HashMap<u16, SystemTime> = HashMap::new();
    while !stopped() {
        std::thread::sleep(Duration::from_secs(2));
        let Some(vt) = active_vt(&tty0) else { continue };
        let Some((uid, input)) = text_login(vt) else { continue };
        let last = unlocked.get(&vt).map_or(input, |&u| u.max(input));
        if last.elapsed().unwrap_or_default() < limit {
            continue;
        }
        let Some(user) = pam::user_name(uid) else { continue };
        eprintln!("matrixclock: tty{vt} ({user}) без ввода {} мин — блокировка", cfg.general.idle_minutes);
        lock_vt(&tty0, vt, cfg.lock.enabled, &exe, &args, &user)?;
        unlocked.insert(vt, SystemTime::now());
    }
    Ok(())
}

fn active_vt(tty0: &File) -> Option<u16> {
    #[repr(C)]
    struct VtStat {
        active: u16,
        signal: u16,
        state: u16,
    }
    let mut s = VtStat { active: 0, signal: 0, state: 0 };
    (unsafe { libc::ioctl(tty0.as_raw_fd(), VT_GETSTATE, &mut s) } == 0).then_some(s.active)
}

/// Консоль в текстовом режиме, в которую вошёл пользователь (не root):
/// (uid владельца терминала, время последнего ввода). X/Wayland (KD_GRAPHICS)
/// и консоли с getty (владелец root) пропускаются.
fn text_login(vt: u16) -> Option<(u32, SystemTime)> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
        .open(format!("/dev/tty{vt}"))
        .ok()?;
    let mut mode: libc::c_int = -1;
    if unsafe { libc::ioctl(f.as_raw_fd(), KDGETMODE, &mut mode) } != 0 || mode != KD_TEXT {
        return None;
    }
    let m = f.metadata().ok()?;
    if m.uid() == 0 {
        return None;
    }
    Some((m.uid(), m.accessed().ok()?))
}

/// Блокировка: свободная консоль, запрет переключения, экран блокировки
/// (перезапускается, если упадёт), затем возврат на консоль пользователя.
fn lock_vt(tty0: &File, back: u16, lock: bool, exe: &Path, args: &[String], user: &str) -> Result<(), String> {
    let fd = tty0.as_raw_fd();
    let mut n: libc::c_int = 0;
    if unsafe { libc::ioctl(fd, VT_OPENQRY, &mut n) } != 0 || n <= 0 {
        return Err("нет свободной консоли (VT_OPENQRY)".into());
    }
    let path = format!("/dev/tty{n}");
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(&path)
        .map_err(io_err(&path))?;
    unsafe {
        libc::ioctl(fd, VT_ACTIVATE, n);
        libc::ioctl(fd, VT_WAITACTIVE, n);
    }
    let switch_locked = lock && unsafe { libc::ioctl(fd, VT_LOCKSWITCH, 0) } == 0;
    loop {
        let (i, o) = (tty.try_clone().map_err(io_err(&path))?, tty.try_clone().map_err(io_err(&path))?);
        let mut cmd = Command::new(exe);
        cmd.args(args).args(["--text", "--user", user]).env("TERM", "linux").stdin(i).stdout(o);
        // своя сессия с этим терминалом как управляющим
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY, 0);
                Ok(())
            });
        }
        let ok = cmd.status().is_ok_and(|s| s.success());
        if ok || !lock || stopped() {
            break;
        }
        eprintln!("matrixclock: экран блокировки завершился с ошибкой — перезапуск");
        std::thread::sleep(Duration::from_secs(2));
    }
    unsafe {
        if switch_locked {
            libc::ioctl(fd, VT_UNLOCKSWITCH, 0);
        }
        libc::ioctl(fd, VT_ACTIVATE, back as libc::c_int);
        libc::ioctl(fd, VT_WAITACTIVE, back as libc::c_int);
    }
    drop(tty);
    unsafe { libc::ioctl(fd, VT_DISALLOCATE, n) };
    Ok(())
}

// ───────────────────────────── регистрация ─────────────────────────────

fn autostart_file() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("не задан HOME")?;
    Ok(base.join("autostart/matrixclock.desktop"))
}

/// Аргумент для строки Exec= (Desktop Entry Specification): в кавычках, % → %%.
/// Символы, требующие многоступенчатого экранирования, не принимаются.
fn exec_arg(s: &str) -> Result<String, String> {
    if s.contains(['"', '`', '$', '\\', '\n']) {
        return Err(format!("недопустимый символ в аргументе для автозапуска: {s}"));
    }
    Ok(format!("\"{}\"", s.replace('%', "%%")))
}

pub fn install(cfg: &Config) -> Result<(), String> {
    if is_root() {
        return Err("--install регистрирует автозапуск для своего пользователя — запускайте без sudo".into());
    }
    let exe = exe()?;
    let mut exec = vec![exec_arg(&exe.to_string_lossy())?, "--daemon".into()];
    for a in args_without("--install") {
        exec.push(exec_arg(&a)?);
    }
    let path = autostart_file()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=MatrixClock\n\
         Comment=Screensaver and screen lock (background mode)\nExec={}\nTerminal=false\n",
        exec.join(" ")
    );
    fs::write(&path, text).map_err(io_err(&path))?;
    println!("Автозапуск записан: {}", path.display());
    println!("Фоновый режим включится при следующем входе в X. Запустить сейчас:");
    println!("  {} --daemon &", exe.display());
    println!("Оконные менеджеры без XDG autostart (i3, dwm, голый startx) этот файл не читают —");
    println!("туда добавьте ту же команду в ~/.xprofile или ~/.xinitrc.");
    if cfg.lock.enabled && !pam::service_exists(&cfg.lock.pam_service) {
        println!("\nВНИМАНИЕ: нет /etc/pam.d/{} — без него блокировка не запустится. Установите:", cfg.lock.pam_service);
        println!("  sudo {} --install-system", exe.display());
    }
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let path = autostart_file()?;
    match fs::remove_file(&path) {
        Ok(()) => println!("Удалён {}. Уже запущенный фоновый процесс работает до выхода из сеанса.", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => println!("Автозапуск не был установлен ({}).", path.display()),
        Err(e) => return Err(io_err(&path)(e)),
    }
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let ok = Command::new("systemctl").args(args).status().is_ok_and(|s| s.success());
    if ok { Ok(()) } else { Err(format!("ошибка: systemctl {}", args.join(" "))) }
}

fn write_file(path: &str, text: &str, mode: u32) -> Result<(), String> {
    fs::write(path, text).map_err(io_err(path))?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(io_err(path))
}

pub fn install_system() -> Result<(), String> {
    if !is_root() {
        return Err("--install-system меняет системные файлы — запускайте через sudo".into());
    }
    let exe = exe()?;
    // копия, а не ссылка: root не должен исполнять файл из каталога, куда пишет
    // пользователь (и Syncthing); кроме того, systemd не подключает юниты из
    // /home на отдельном разделе
    // /usr/bin — из пакета, уже системная: её и берём
    let bin = if exe == Path::new(PKG_BIN) || exe == Path::new(BIN) {
        exe.display().to_string()
    } else {
        let tmp = format!("{BIN}.new");
        fs::copy(&exe, &tmp).map_err(io_err(&tmp))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(io_err(&tmp))?;
        fs::rename(&tmp, BIN).map_err(io_err(BIN))?;
        println!("Программа: {} → {BIN}", exe.display());
        BIN.to_string()
    };
    if Path::new(PAM_FILE).exists() {
        println!("PAM: {PAM_FILE} уже есть — не трогаю");
    } else {
        write_file(PAM_FILE, PAM_TEXT, 0o644)?;
        println!("PAM: {PAM_FILE}");
    }
    write_file(UNIT, &UNIT_TEXT.replace("@BIN@", &bin), 0o644)?;
    println!("Служба: {UNIT}");
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", SERVICE])?;
    systemctl(&["restart", SERVICE])?;
    println!("Служба включена и запущена. Состояние: systemctl status {SERVICE}");
    println!("Настройки службы: /etc/matrixclock/config.toml (можно скопировать свой ~/.config/matrixclock/config.toml).");
    Ok(())
}

pub fn uninstall_system() -> Result<(), String> {
    if !is_root() {
        return Err("--uninstall-system меняет системные файлы — запускайте через sudo".into());
    }
    if Path::new(UNIT).exists() {
        systemctl(&["disable", "--now", SERVICE])?;
        fs::remove_file(UNIT).map_err(io_err(UNIT))?;
        systemctl(&["daemon-reload"])?;
        println!("Служба остановлена и удалена: {UNIT}");
    }
    match fs::remove_file(BIN) {
        Ok(()) => println!("Удалена {BIN}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_err(BIN)(e)),
    }
    println!("{PAM_FILE} оставлен: он нужен для блокировки в X. Удалить: sudo rm {PAM_FILE}");
    Ok(())
}
