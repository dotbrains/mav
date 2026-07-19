use minidumper::{LoopAction, MinidumpBinary};
use parking_lot::Mutex;
use std::{
    fs::{self, File},
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::{CrashInfo, CrashPanic, CrashServerMessage, InitCrashHandler, UserInfo};

pub struct CrashServer {
    initialization_params: Mutex<Option<InitCrashHandler>>,
    panic_info: Mutex<Option<CrashPanic>>,
    active_gpu: Mutex<Option<system_specs::GpuSpecs>>,
    user_info: Mutex<Option<UserInfo>>,
    has_connection: Arc<AtomicBool>,
    logs_dir: PathBuf,
}

impl CrashServer {
    pub fn new(has_connection: Arc<AtomicBool>, logs_dir: PathBuf) -> Self {
        Self {
            initialization_params: Mutex::default(),
            panic_info: Mutex::default(),
            user_info: Mutex::default(),
            has_connection,
            active_gpu: Mutex::default(),
            logs_dir,
        }
    }
}

impl minidumper::ServerHandler for CrashServer {
    fn create_minidump_file(&self) -> Result<(File, PathBuf), io::Error> {
        let dump_path = self
            .logs_dir
            .join(
                &self
                    .initialization_params
                    .lock()
                    .as_ref()
                    .expect("Missing initialization data")
                    .session_id,
            )
            .with_extension("dmp");
        let file = File::create(&dump_path)?;
        Ok((file, dump_path))
    }

    fn on_minidump_created(&self, result: Result<MinidumpBinary, minidumper::Error>) -> LoopAction {
        let minidump_error = match result {
            Ok(MinidumpBinary { mut file, path, .. }) => {
                use io::Write;
                file.flush().ok();
                // TODO: clean this up once https://github.com/EmbarkStudios/crash-handling/issues/101 is addressed
                drop(file);
                let original_file = File::open(&path).unwrap();
                let compressed_path = path.with_extension("zstd");
                let compressed_file = File::create(&compressed_path).unwrap();
                zstd::stream::copy_encode(original_file, compressed_file, 0).ok();
                fs::rename(&compressed_path, path).unwrap();
                None
            }
            Err(e) => Some(format!("{e:?}")),
        };

        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let gpus = vec![];

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let gpus = match system_specs::read_gpu_info_from_sys_class_drm() {
            Ok(gpus) => gpus,
            Err(err) => {
                log::warn!("Failed to collect GPU information for crash report: {err}");
                vec![]
            }
        };

        let crash_info = CrashInfo {
            init: self
                .initialization_params
                .lock()
                .clone()
                .expect("not initialized"),
            panic: self.panic_info.lock().clone(),
            minidump_error,
            active_gpu: self.active_gpu.lock().clone(),
            gpus,
            user_info: self.user_info.lock().clone(),
        };

        let crash_data_path = self
            .logs_dir
            .join(&crash_info.init.session_id)
            .with_extension("json");

        fs::write(crash_data_path, serde_json::to_vec(&crash_info).unwrap()).ok();

        LoopAction::Exit
    }

    fn on_message(&self, _: u32, buffer: Vec<u8>) {
        let message: CrashServerMessage =
            serde_json::from_slice(&buffer).expect("invalid init data");
        match message {
            CrashServerMessage::Init(init_data) => {
                self.initialization_params.lock().replace(init_data);
            }
            CrashServerMessage::Panic(crash_panic) => {
                self.panic_info.lock().replace(crash_panic);
            }
            CrashServerMessage::GPUInfo(gpu_specs) => {
                self.active_gpu.lock().replace(gpu_specs);
            }
            CrashServerMessage::UserInfo(user_info) => {
                self.user_info.lock().replace(user_info);
            }
        }
    }

    fn on_client_disconnected(&self, _clients: usize) -> LoopAction {
        LoopAction::Exit
    }

    fn on_client_connected(&self, _clients: usize) -> LoopAction {
        self.has_connection.store(true, Ordering::SeqCst);
        LoopAction::Continue
    }
}
