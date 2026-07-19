mod server;

use crash_handler::{CrashEventResult, CrashHandler};
use log::info;
use minidumper::{Server, SocketName};
use serde::{Deserialize, Serialize};
use std::{panic::Location, pin::Pin};

use system_specs::GpuSpecs;

use std::{
    env, panic,
    path::{Path, PathBuf},
    process::{self},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub use minidumper::Client;
use server::CrashServer;

const CRASH_HANDLER_PING_TIMEOUT: Duration = Duration::from_secs(60);
const CRASH_HANDLER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Force a backtrace to be printed on panic.
pub fn force_backtrace() {
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        unsafe { env::set_var("RUST_BACKTRACE", "1") };
        old_hook(info);
        // prevent the macOS crash dialog from popping up
        if cfg!(target_os = "macos") {
            std::process::exit(1);
        }
    }));
}

/// Install crash signal handlers and spawn the crash-handler subprocess.
///
/// All work happens lazily in the returned future, so it runs on whichever
/// executor polls it. The keepalive task is passed to `spawn` so the caller
/// decides which executor to schedule it on.
pub fn init<F, S, C, P>(
    crash_init: InitCrashHandler,
    spawn: S,
    socket_path: P,
    wait_timer: C,
) -> impl Future<Output = Arc<Client>> + use<F, C, S, P>
where
    F: Future<Output = ()> + Send + Sync + 'static,
    C: (Fn(Duration) -> F) + Send + Sync + 'static,
    S: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
    P: FnOnce(u32) -> PathBuf,
{
    connect_and_keepalive(crash_init, socket_path, wait_timer, spawn)
}

/// Spawn the crash-handler subprocess, connect the IPC client, and run the
/// keepalive ping loop. This is the future returned by [`init`], so it runs on
/// whichever executor the caller polls it with.
async fn connect_and_keepalive<F, C, S, P>(
    crash_init: InitCrashHandler,
    socket_path: P,
    wait_timer: C,
    spawn: S,
) -> Arc<Client>
where
    F: Future<Output = ()> + Send + Sync + 'static,
    C: (Fn(Duration) -> F) + Send + Sync + 'static,
    S: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
    P: FnOnce(u32) -> PathBuf,
{
    let exe = env::current_exe().expect("unable to find ourselves");
    let socket_path = socket_path(process::id());
    let mut _crash_handler = spawn_crash_handler(&exe, &socket_path);
    info!("spawning crash handler process");
    let mut elapsed = Duration::ZERO;
    let retry_frequency = Duration::from_millis(100);
    let client = loop {
        if let Ok(client) = Client::with_name(SocketName::Path(&socket_path)) {
            info!("connected to crash handler process after {elapsed:?}");
            break client;
        }
        elapsed += retry_frequency;
        wait_timer(retry_frequency).await;
    };
    let client = Arc::new(client);

    panic::set_hook({
        let client = client.clone();
        Box::new(move |payload| {
            panic_hook(
                client.clone(),
                payload.payload_as_str().unwrap_or("Box<Any>"),
                payload.location(),
            )
        })
    });
    info!("panic handler registered");
    let handler = CrashHandler::attach(unsafe {
        let client = client.clone();
        let handler = move |crash_context: &crash_handler::CrashContext| {
            // set when the first minidump request is made to avoid generating duplicate crash reports
            static REQUESTED_MINIDUMP: AtomicBool = AtomicBool::new(false);

            // only request a minidump once
            let res = if REQUESTED_MINIDUMP
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                #[cfg(target_os = "macos")]
                macos::suspend_all_other_threads();

                // on macos this "ping" is needed to ensure that all our
                // `client.send_message` calls have been processed before we trigger the
                // minidump request.
                client.ping().ok();
                let r = client.request_dump(crash_context);
                if let Err(e) = &r {
                    eprintln!("failed to request dump: {:?}", e);
                }
                #[cfg(target_os = "macos")]
                macos::resume_all_other_threads();
                r.is_ok()
            } else {
                true
            };
            CrashEventResult::Handled(res)
        };
        crash_handler::make_crash_event(handler)
    })
    .expect("failed to attach signal handler");

    info!("crash signal handlers installed");
    send_crash_server_message(&client, CrashServerMessage::Init(crash_init));

    #[cfg(target_os = "linux")]
    handler.set_ptracer(Some(_crash_handler.id()));

    info!("crash handler registered");
    spawn(Box::pin({
        let client = client.clone();
        async move {
            let _handler = { handler };
            loop {
                if let Err(e) = client.ping() {
                    #[cfg(not(target_os = "windows"))]
                    log::error!(
                        "ping failed: {:?}, process exit status: {:?}",
                        e,
                        _crash_handler.try_status()
                    );
                    #[cfg(target_os = "windows")]
                    log::error!("ping failed: {:?}", e,);
                    break;
                };
                wait_timer(Duration::from_secs(10)).await;
            }
        }
    }));
    client
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CrashInfo {
    pub init: InitCrashHandler,
    pub panic: Option<CrashPanic>,
    pub minidump_error: Option<String>,
    pub gpus: Vec<system_specs::GpuInfo>,
    pub active_gpu: Option<system_specs::GpuSpecs>,
    pub user_info: Option<UserInfo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InitCrashHandler {
    pub session_id: String,
    pub mav_version: String,
    pub binary: String,
    pub release_channel: String,
    pub commit_sha: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CrashPanic {
    pub message: String,
    pub span: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UserInfo {
    pub metrics_id: Option<String>,
    pub is_staff: Option<bool>,
}

fn send_crash_server_message(crash_client: &Arc<Client>, message: CrashServerMessage) {
    let data = match serde_json::to_vec(&message) {
        Ok(data) => data,
        Err(err) => {
            log::warn!("Failed to serialize crash server message: {:?}", err);
            return;
        }
    };

    if let Err(err) = crash_client.send_message(0, data) {
        log::warn!("Failed to send data to crash server {:?}", err);
    }
}

pub fn set_gpu_info(crash_client: &Arc<Client>, specs: GpuSpecs) {
    send_crash_server_message(crash_client, CrashServerMessage::GPUInfo(specs));
}

pub fn set_user_info(crash_client: &Arc<Client>, info: UserInfo) {
    send_crash_server_message(crash_client, CrashServerMessage::UserInfo(info));
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum CrashServerMessage {
    Init(InitCrashHandler),
    Panic(CrashPanic),
    GPUInfo(GpuSpecs),
    UserInfo(UserInfo),
}

/// Rust's string-slicing panics embed the user's string content in the message,
/// e.g. "byte index 4 is out of bounds of `a`". Strip that suffix so we
/// don't upload arbitrary user text in crash reports.
fn strip_user_string_from_panic(message: &str) -> String {
    const STRING_PANIC_PREFIXES: &[&str] = &[
        // Older rustc (pre-1.95):
        "byte index ",
        "begin <= end (",
        // Newer rustc (1.95+):
        // https://github.com/rust-lang/rust/pull/145024
        "start byte index ",
        "end byte index ",
        "begin > end (",
    ];

    if (message.ends_with('`') || message.ends_with("`[...]"))
        && STRING_PANIC_PREFIXES
            .iter()
            .any(|prefix| message.starts_with(prefix))
        && let Some(open) = message.find('`')
    {
        return format!("{} `<redacted>`", &message[..open]);
    }
    message.to_owned()
}

pub fn panic_hook(crash_client: Arc<Client>, message: &str, location: Option<&Location>) {
    let message = strip_user_string_from_panic(message);

    let span = location
        .map(|loc| format!("{}:{}", loc.file(), loc.line()))
        .unwrap_or_default();

    let current_thread = std::thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>");

    let location = location.map_or_else(|| "<unknown>".to_owned(), |location| location.to_string());
    log::error!("thread '{thread_name}' panicked at {location}:\n{message}...");

    send_crash_server_message(
        &crash_client,
        CrashServerMessage::Panic(CrashPanic { message, span }),
    );
    log::error!("triggering a crash to generate a minidump...");

    #[cfg(target_os = "macos")]
    macos::set_panic_thread_id();
    #[cfg(target_os = "windows")]
    {
        // https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-
        CrashHandler.simulate_exception(Some(234)); // (MORE_DATA_AVAILABLE)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::abort();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    static PANIC_THREAD_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    pub(super) fn set_panic_thread_id() {
        PANIC_THREAD_ID.store(
            unsafe { mach2::mach_init::mach_thread_self() },
            std::sync::atomic::Ordering::Release,
        );
    }

    pub(super) unsafe fn suspend_all_other_threads() {
        let task = unsafe { mach2::traps::current_task() };
        let mut threads: mach2::mach_types::thread_act_array_t = std::ptr::null_mut();
        let mut count = 0;
        unsafe {
            mach2::task::task_threads(task, &raw mut threads, &raw mut count);
        }
        let current = unsafe { mach2::mach_init::mach_thread_self() };
        for i in 0..count {
            let t = unsafe { *threads.add(i as usize) };
            if t != current {
                unsafe { mach2::thread_act::thread_suspend(t) };
            }
        }
    }

    pub(super) unsafe fn resume_all_other_threads() {
        let task = unsafe { mach2::traps::current_task() };
        let mut threads: mach2::mach_types::thread_act_array_t = std::ptr::null_mut();
        let mut count = 0;
        unsafe {
            mach2::task::task_threads(task, &raw mut threads, &raw mut count);
        }
        let current = unsafe { mach2::mach_init::mach_thread_self() };
        for i in 0..count {
            let t = unsafe { *threads.add(i as usize) };
            if t != current {
                unsafe { mach2::thread_act::thread_resume(t) };
            }
        }
    }
}
#[cfg(not(target_os = "windows"))]
fn spawn_crash_handler(exe: &Path, socket_name: &Path) -> async_process::Child {
    async_process::Command::new(exe)
        .arg("--crash-handler")
        .arg(&socket_name)
        .spawn()
        .expect("unable to spawn server process")
}

#[cfg(target_os = "windows")]
fn spawn_crash_handler(exe: &Path, socket_name: &Path) {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Threading::{
        CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTF_FORCEOFFFEEDBACK,
        STARTUPINFOW,
    };
    use windows::core::PWSTR;

    let mut command_line: Vec<u16> = OsStr::new(&format!(
        "\"{}\" --crash-handler \"{}\"",
        exe.display(),
        socket_name.display()
    ))
    .encode_wide()
    .chain(once(0))
    .collect();

    let mut startup_info = STARTUPINFOW::default();
    startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;

    // By default, Windows enables a "busy" cursor when a GUI application is launched.
    // This cursor is disabled once the application starts processing window messages.
    // Since the crash handler process doesn't process messages, this "busy" cursor stays enabled for a long time.
    // Disable the cursor feedback to prevent this from happening.
    startup_info.dwFlags = STARTF_FORCEOFFFEEDBACK;

    let mut process_info = PROCESS_INFORMATION::default();

    unsafe {
        CreateProcessW(
            None,
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            None,
            &startup_info,
            &mut process_info,
        )
        .expect("unable to spawn server process");

        windows::Win32::Foundation::CloseHandle(process_info.hProcess).ok();
        windows::Win32::Foundation::CloseHandle(process_info.hThread).ok();
    }
}

pub fn crash_server(socket: &Path, logs_dir: PathBuf) {
    let Ok(mut server) = Server::with_name(SocketName::Path(socket)) else {
        log::info!("Couldn't create socket, there may already be a running crash server");
        return;
    };

    let shutdown = Arc::new(AtomicBool::new(false));
    let has_connection = Arc::new(AtomicBool::new(false));

    thread::Builder::new()
        .name("CrashServerTimeout".to_owned())
        .spawn({
            let shutdown = shutdown.clone();
            let has_connection = has_connection.clone();
            move || {
                std::thread::sleep(CRASH_HANDLER_CONNECT_TIMEOUT);
                if !has_connection.load(Ordering::SeqCst) {
                    shutdown.store(true, Ordering::SeqCst);
                }
            }
        })
        .unwrap();

    server
        .run(
            Box::new(CrashServer::new(has_connection, logs_dir)),
            &shutdown,
            Some(CRASH_HANDLER_PING_TIMEOUT),
        )
        .expect("failed to run server");
}
