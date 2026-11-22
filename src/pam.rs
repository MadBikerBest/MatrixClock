//! Минимальная обвязка libpam: только проверка пароля (pam_authenticate).
//! Контракт conversation-функции — man pam_conv(3).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use zeroize::Zeroizing;

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_DISALLOW_NULL_AUTHTOK: c_int = 0x0001;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type ConvFn = unsafe extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int;

#[repr(C)]
struct PamConv {
    conv: ConvFn,
    appdata_ptr: *mut c_void,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conv: *const PamConv, pamh: *mut *mut c_void) -> c_int;
    fn pam_authenticate(pamh: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut c_void, status: c_int) -> c_int;
}

/// На каждый запрос пароля отвечаем копией пароля (malloc — освобождает модуль),
/// на информационные сообщения — пустым ответом.
unsafe extern "C" fn conversation(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata: *mut c_void,
) -> c_int {
    unsafe {
        if num_msg <= 0 {
            return PAM_BUF_ERR;
        }
        let n = num_msg as usize;
        let out = libc::calloc(n, size_of::<PamResponse>()) as *mut PamResponse;
        if out.is_null() {
            return PAM_BUF_ERR;
        }
        let password = appdata as *const c_char;
        for i in 0..n {
            let m = *msg.add(i); // Linux-PAM: массив указателей на сообщения
            let style = (*m).msg_style;
            if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
                let p = libc::strdup(password);
                if p.is_null() {
                    for j in 0..i {
                        libc::free((*out.add(j)).resp as *mut c_void);
                    }
                    libc::free(out as *mut c_void);
                    return PAM_BUF_ERR;
                }
                (*out.add(i)).resp = p;
            }
        }
        *resp = out;
        PAM_SUCCESS
    }
}

/// Проверяет пароль пользователя через сервис PAM. Блокирующий вызов
/// (pam_unix делает задержку ~2 с при ошибке) — вызывать из отдельного потока.
pub fn authenticate(service: &str, user: &str, password: &[u8]) -> bool {
    let (Ok(service), Ok(user)) = (CString::new(service), CString::new(user)) else {
        return false;
    };
    let mut buf = Zeroizing::new(Vec::with_capacity(password.len() + 1));
    buf.extend_from_slice(password);
    if buf.contains(&0) {
        return false;
    }
    buf.push(0);
    let conv = PamConv { conv: conversation, appdata_ptr: buf.as_ptr() as *mut c_void };
    let mut handle: *mut c_void = std::ptr::null_mut();
    unsafe {
        let rc = pam_start(service.as_ptr(), user.as_ptr(), &conv, &mut handle);
        if rc != PAM_SUCCESS {
            return false;
        }
        let rc = pam_authenticate(handle, PAM_DISALLOW_NULL_AUTHTOK);
        pam_end(handle, rc);
        rc == PAM_SUCCESS
    }
}

/// Есть ли файл сервиса PAM. Без него Linux-PAM берёт "other" (на Arch — pam_deny),
/// и разблокировать будет невозможно.
pub fn service_exists(service: &str) -> bool {
    ["/etc/pam.d", "/usr/lib/pam.d"].iter().any(|d| std::path::Path::new(d).join(service).exists())
}

/// Имя пользователя по uid.
pub fn user_name(uid: libc::uid_t) -> Option<String> {
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let mut buf = vec![0 as c_char; 4096];
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result) };
    if rc != 0 || result.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pwd.pw_name) }.to_str().ok().map(String::from)
}
