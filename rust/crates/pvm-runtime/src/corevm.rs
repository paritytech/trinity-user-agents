/* SPDX-License-Identifier: Apache-2.0 OR MIT
 * Vendored from paritytech/polkavm examples/quake/src/vm.rs at
 * 3df1d0309c4c81a1aad0a755d83570d203bba1d9 and adapted for Epoca.
 */

#![allow(non_upper_case_globals)]

use polkavm::{
    Config, Engine, GasMeteringKind, InterruptKind, MemoryAccessError, Module, ModuleConfig,
    ProgramBlob, ProgramCounter, RawInstance, Reg,
};
use std::collections::{BTreeMap, VecDeque};
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::time::Instant;

struct File {
    blob: Vec<u8>,
}

struct Fd {
    file: Arc<File>,
    position: u64,
}

struct OpenFiles {
    descriptors: BTreeMap<u64, Fd>,
    next: u64,
}

impl OpenFiles {
    fn new() -> Self {
        Self {
            descriptors: BTreeMap::new(),
            next: 3,
        }
    }

    fn open(&mut self, file: Arc<File>) -> Result<u64, u64> {
        if self.descriptors.len() >= MAX_OPEN_FILES {
            return Err(EMFILE);
        }
        let fd = self.next;
        self.next = self.next.checked_add(1).ok_or(EMFILE)?;
        self.descriptors.insert(fd, Fd { file, position: 0 });
        Ok(fd)
    }

    fn get_mut(&mut self, fd: u64) -> Option<&mut Fd> {
        self.descriptors.get_mut(&fd)
    }

    fn remove(&mut self, fd: u64) -> Option<Fd> {
        self.descriptors.remove(&fd)
    }
}

pub struct Vm {
    start: ProgramCounter,
    instance: RawInstance,
    backend: polkavm::BackendKind,
    filesystem: BTreeMap<Vec<u8>, Arc<File>>,
    open_files: OpenFiles,
    input_events: VecDeque<InputEvent>,
    audio_channels: u32,
    epoca_input_events: VecDeque<[u8; crate::INPUT_EVENT_BYTES]>,
    started: Instant,

    import_syscall: Option<u32>,
    import_set_palette: Option<u32>,
    import_display: Option<u32>,
    import_fetch_inputs: Option<u32>,
    import_init_audio: Option<u32>,
    import_output_audio: Option<u32>,
    import_epoca_inputs: Option<u32>,
    import_epoca_audio: Option<u32>,
    import_asset_read: Option<u32>,
    import_time_ms: Option<u32>,
    import_log: Option<u32>,
    import_yield: Option<u32>,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct InputEvent {
    key: u8,
    value: u8,
}

const SYS_read: u64 = 63;
const SYS_readv: u64 = 65;
const SYS_writev: u64 = 66;
const SYS_exit: u64 = 93;
const SYS_openat: u64 = 56;
const SYS_lseek: u64 = 62;
const SYS_close: u64 = 57;
const SEEK_SET: u64 = 0;
const SEEK_CUR: u64 = 1;
const SEEK_END: u64 = 2;
const FILENO_STDOUT: u64 = 1;
const FILENO_STDERR: u64 = 2;
const ENOSYS: u64 = 38;
const EFAULT: u64 = 14;
const ENOENT: u64 = 2;
const EBADF: u64 = 9;
const EACCES: u64 = 13;
const EINVAL: u64 = 22;
const EMFILE: u64 = 24;
const AT_FDCWD: u64 = (-100_i64) as u64;
const IOV_MAX: u64 = 1024;
const MAX_QUEUED_INPUT_EVENTS: usize = 256;
const MAX_GUEST_WRITE_BYTES: u64 = 4 * 1024;
const MAX_OPEN_FILES: usize = 256;
const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const AT_PAGESZ: u64 = 6;

fn errno(error: u64) -> u64 {
    (-(error as i64)) as u64
}

fn normalize_path(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn seek_position(current: u64, length: u64, offset: i64, whence: u64) -> Result<u64, u64> {
    let base = match whence {
        SEEK_SET => 0,
        SEEK_CUR => current,
        SEEK_END => length,
        _ => return Err(EINVAL),
    };
    u64::try_from(i128::from(base) + i128::from(offset)).map_err(|_| EINVAL)
}

fn queued_input_chunks(
    events: &VecDeque<InputEvent>,
    limit: usize,
) -> (&[InputEvent], &[InputEvent]) {
    let remaining = limit.min(events.len());
    let (first, second) = events.as_slices();
    let first = &first[..first.len().min(remaining)];
    let second = &second[..second.len().min(remaining - first.len())];
    (first, second)
}

fn input_destination(address: u64, event_offset: usize) -> Result<u32, String> {
    let byte_offset = event_offset
        .checked_mul(core::mem::size_of::<InputEvent>())
        .and_then(|offset| u64::try_from(offset).ok())
        .ok_or_else(|| "input address overflow".to_owned())?;
    address
        .checked_add(byte_offset)
        .and_then(|address| u32::try_from(address).ok())
        .ok_or_else(|| "input address is out of range".to_owned())
}

pub enum Interruption {
    Exit,
    Yield,
    SetPalette {
        palette: Vec<u8>,
    },
    Display {
        width: u64,
        height: u64,
        framebuffer: Vec<u8>,
    },
    AudioInit {
        channels: u32,
        sample_rate: u32,
    },
    AudioFrame {
        buffer: Vec<i16>,
    },
}

impl Vm {
    pub fn from_blob(
        blob: ProgramBlob,
        backend: polkavm::BackendKind,
    ) -> Result<Self, polkavm::Error> {
        let mut config = Config::new();
        config.set_backend(Some(backend));
        config.set_sandboxing_enabled(true);
        #[cfg(target_os = "macos")]
        {
            config.set_sandbox(Some(polkavm::SandboxKind::Generic));
            config.set_allow_experimental(true);
        }
        let engine = Engine::new(&config)?;
        let backend = engine.backend();
        let mut module_config = ModuleConfig::new();
        module_config.set_gas_metering(Some(GasMeteringKind::Sync));
        #[cfg(not(target_arch = "wasm32"))]
        module_config.set_max_heap_size(Some(crate::MAX_GUEST_HEAP_BYTES));
        #[cfg(target_os = "macos")]
        module_config.set_page_size(16_384);
        let module = Module::from_blob(&engine, &module_config, blob)?;

        let start = module
            .exports()
            .find(|export| export.symbol() == "_pvm_start")
            .ok_or_else(|| "'_pvm_start' export not found".to_string())?
            .program_counter();

        let mut import_syscall = None;
        let mut import_set_palette = None;
        let mut import_display = None;
        let mut import_fetch_inputs = None;
        let mut import_init_audio = None;
        let mut import_output_audio = None;
        let mut import_epoca_inputs = None;
        let mut import_epoca_audio = None;
        let mut import_asset_read = None;
        let mut import_time_ms = None;
        let mut import_log = None;
        let mut import_yield = None;

        for (import_index, import) in module.imports().into_iter().enumerate() {
            let Some(import) = import else {
                continue;
            };

            let import_index = import_index as u32;
            match import.as_bytes() {
                b"pvm_syscall" => import_syscall = Some(import_index),
                b"pvm_set_palette" => import_set_palette = Some(import_index),
                b"pvm_display" => import_display = Some(import_index),
                b"pvm_fetch_inputs" => import_fetch_inputs = Some(import_index),
                b"pvm_init_audio" => import_init_audio = Some(import_index),
                b"pvm_output_audio" => import_output_audio = Some(import_index),
                b"pvm_fetch_epoca_inputs" => import_epoca_inputs = Some(import_index),
                b"host_audio_submit" => import_epoca_audio = Some(import_index),
                b"pvm_asset_read" => import_asset_read = Some(import_index),
                b"pvm_time_ms" => import_time_ms = Some(import_index),
                b"host_log" => import_log = Some(import_index),
                b"pvm_yield" => import_yield = Some(import_index),
                _ => return Err(format!("unsupported import: {}", import).into()),
            }
        }

        let mut instance = module.instantiate()?;
        instance.set_interpreter_guest_memory_limit(Some(crate::MAX_GUEST_HEAP_BYTES as usize));
        Ok(Self {
            start,
            instance,
            backend,
            filesystem: BTreeMap::new(),
            open_files: OpenFiles::new(),
            input_events: VecDeque::with_capacity(MAX_QUEUED_INPUT_EVENTS),
            audio_channels: 0,
            epoca_input_events: VecDeque::with_capacity(MAX_QUEUED_INPUT_EVENTS),
            started: Instant::now(),
            import_syscall,
            import_set_palette,
            import_display,
            import_fetch_inputs,
            import_init_audio,
            import_output_audio,
            import_epoca_inputs,
            import_epoca_audio,
            import_asset_read,
            import_time_ms,
            import_log,
            import_yield,
        })
    }

    pub fn backend(&self) -> polkavm::BackendKind {
        self.backend
    }

    pub fn set_gas(&mut self, gas: u64) {
        self.instance.set_gas(gas.min(i64::MAX as u64) as i64);
    }
    pub fn gas_remaining(&self) -> u64 {
        self.instance.gas().max(0) as u64
    }

    fn send_input_event(&mut self, key: u8, value: u8) {
        if key == crate::quake_keys::MOUSE_X || key == crate::quake_keys::MOUSE_Y {
            if let Some(event) = self.input_events.iter_mut().find(|event| event.key == key) {
                event.value = (event.value as i8).saturating_add(value as i8) as u8;
                return;
            }
        }

        if self.input_events.len() == MAX_QUEUED_INPUT_EVENTS {
            self.input_events.pop_front();
        }
        self.input_events.push_back(InputEvent { key, value });
    }

    pub fn send_key(&mut self, key: u8, is_pressed: bool) {
        self.send_input_event(key, if is_pressed { 1 } else { 0 });
    }

    pub fn send_mouse_move(&mut self, delta_x: i8, delta_y: i8) {
        if delta_x != 0 {
            self.send_input_event(crate::quake_keys::MOUSE_X, delta_x as u8);
        }

        if delta_y != 0 {
            self.send_input_event(crate::quake_keys::MOUSE_Y, delta_y as u8);
        }
    }
    pub fn uses_epoca_inputs(&self) -> bool {
        self.import_epoca_inputs.is_some()
    }

    pub fn send_epoca_input(&mut self, event: crate::InputEvent) {
        if self.epoca_input_events.len() == MAX_QUEUED_INPUT_EVENTS {
            self.epoca_input_events.pop_front();
        }
        self.epoca_input_events.push_back(event.encode());
    }

    fn read_cstr(&mut self, address: u64) -> Result<Option<Vec<u8>>, String> {
        // FIXME: This is slow.
        let mut buffer = Vec::new();
        for offset in 0..255 {
            let Some(address) = address
                .checked_add(offset)
                .and_then(|address| u32::try_from(address).ok())
            else {
                return Ok(None);
            };
            match self.instance.read_u8(address) {
                Ok(byte) => {
                    if byte == 0 {
                        return Ok(Some(buffer));
                    }

                    buffer.push(byte)
                }
                Err(MemoryAccessError::Error(error)) => return Err(error.into()),
                Err(MemoryAccessError::OutOfRangeAccess { .. }) => return Ok(None),
                Err(MemoryAccessError::MemoryLimitReached) => return Ok(None),
            }
        }

        Ok(None)
    }

    pub fn register_file(&mut self, path: &str, blob: Vec<u8>) {
        let normalized = normalize_path(path);
        self.filesystem
            .insert(normalized.into_bytes(), Arc::new(File { blob }));
    }

    fn handle_open(&mut self, path: &[u8], flags: u64) -> u64 {
        let path = normalize_path(&String::from_utf8_lossy(path));
        log::debug!("Open: path={path:?}, flags=0x{flags:x}");

        if let Some(file) = self.filesystem.get(path.as_bytes()) {
            if (flags & (O_WRONLY | O_RDWR)) != 0 {
                return errno(EACCES);
            }

            return match self.open_files.open(Arc::clone(file)) {
                Ok(fd) => fd,
                Err(error) => errno(error),
            };
        }

        errno(ENOENT)
    }

    fn handle_lseek(&mut self, fd: u64, offset: i64, whence: u64) -> u64 {
        log::trace!("Seek: fd={fd}, offset={offset}, whence={whence}");

        let Some(fd) = self.open_files.get_mut(fd) else {
            log::trace!("  -> BADF");
            return errno(EBADF);
        };

        let Ok(position) = seek_position(fd.position, fd.file.blob.len() as u64, offset, whence)
        else {
            log::trace!("  -> EINVAL");
            return errno(EINVAL);
        };
        fd.position = position;

        log::trace!("  -> offset={}", fd.position);
        fd.position
    }

    fn handle_read(&mut self, fd: u64, address: u64, length: u64) -> Result<u64, String> {
        log::trace!("Read: fd={fd}, address=0x{address:x}, length={length}");

        let Some(fd) = self.open_files.get_mut(fd) else {
            log::trace!("  -> EBADF");
            return Ok(errno(EBADF));
        };

        if address.checked_add(length).is_none() || u32::try_from(address + length).is_err() {
            log::trace!("  -> EFAULT");
            return Ok(errno(EFAULT));
        }

        let Ok(address) = u32::try_from(address) else {
            log::trace!("  -> EFAULT");
            return Ok(errno(EFAULT));
        };

        let end = core::cmp::min(fd.position.wrapping_add(length), fd.file.blob.len() as u64);
        if fd.position >= end || fd.position >= fd.file.blob.len() as u64 {
            log::trace!("  -> offset={}, length=0", fd.position);
            return Ok(0);
        }

        let blob = &fd.file.blob[fd.position as usize..end as usize];
        match self.instance.write_memory(address, blob) {
            Ok(()) => {}
            Err(MemoryAccessError::Error(error)) => return Err(error.into()),
            Err(MemoryAccessError::OutOfRangeAccess { .. }) => {
                log::trace!("  -> EFAULT");
                return Ok(errno(EFAULT));
            }
            Err(MemoryAccessError::MemoryLimitReached) => return Ok(errno(EFAULT)),
        }

        let length_out = blob.len() as u64;
        log::trace!(
            "  -> offset={}, length={}, new offset={}",
            fd.position,
            length_out,
            fd.position + length_out
        );

        fd.position += length_out;
        Ok(length_out)
    }

    fn handle_write(&mut self, fd: u64, address: u64, length: u64) -> Result<u64, String> {
        if fd != FILENO_STDOUT && fd != FILENO_STDERR {
            return Ok(errno(EBADF));
        }

        let length = length.min(MAX_GUEST_WRITE_BYTES);
        if address.checked_add(length).is_none() || u32::try_from(address + length).is_err() {
            return Ok(errno(EFAULT));
        }

        let Ok(address) = u32::try_from(address) else {
            return Ok(errno(EFAULT));
        };

        let data = match self.instance.read_memory(address, length as u32) {
            Ok(data) => data,
            Err(MemoryAccessError::Error(error)) => return Err(error.into()),
            Err(MemoryAccessError::OutOfRangeAccess { .. })
            | Err(MemoryAccessError::MemoryLimitReached) => return Ok(errno(EFAULT)),
        };
        eprint!("{}", String::from_utf8_lossy(&data));
        Ok(length)
    }

    fn handle_close(&mut self, fd: u64) -> u64 {
        log::debug!("Close: fd = {fd}");
        let Some(_fd) = self.open_files.remove(fd) else {
            log::trace!("  -> EBADF");
            return errno(EBADF);
        };

        0
    }

    #[allow(non_upper_case_globals)]
    pub fn setup<'a, I>(&mut self, argv: I) -> Result<(), String>
    where
        I: IntoIterator<Item = &'a str>,
        <I as IntoIterator>::IntoIter: ExactSizeIterator,
    {
        let argv = argv.into_iter();
        let argc = argv.len() as u64;
        let envp: &[&str] = &[];
        let envp_len = envp.len() as u64;
        let auxv: &[(u64, u64)] = &[(AT_PAGESZ, 4096)];
        let auxv_len = auxv.len() as u64;

        let mut sp = self.instance.module().default_sp();

        sp -= (1 + argc + 1 + envp_len + 1 + (auxv_len + 1) * 2) * 8;
        let address_init = sp;

        let mut p = sp;
        self.instance.write_u64(p as u32, argc)?;
        p += 8;

        for arg in argv {
            sp -= arg.len() as u64 + 1;
            self.instance.write_memory(sp as u32, arg.as_bytes())?;
            self.instance.write_u64(p as u32, sp)?;
            p += 8;
        }
        p += 8; // Null pointer.

        for arg in envp {
            sp -= arg.len() as u64 + 1;
            self.instance.write_memory(sp as u32, arg.as_bytes())?;
            self.instance.write_u64(p as u32, sp)?;
            p += 8;
        }
        p += 8; // Null pointer.

        for &(key, value) in auxv {
            self.instance.write_u64(p as u32, key)?;
            p += 8;
            self.instance.write_u64(p as u32, value)?;
            p += 8;
        }

        self.instance.set_reg(Reg::SP, sp);
        self.instance.set_reg(Reg::A0, address_init);
        self.instance.set_reg(Reg::RA, polkavm::RETURN_TO_HOST);
        self.instance.set_next_program_counter(self.start);
        Ok(())
    }

    pub fn run(&mut self) -> Result<Interruption, String> {
        'outer_loop: loop {
            #[allow(clippy::redundant_guards)] // Disable buggy lint.
            match self.instance.run()? {
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_set_palette => {
                    let address = self.instance.reg(Reg::A0);
                    log::debug!("Set palette called: 0x{:x}", address);
                    let palette = self.instance.read_memory(address as u32, 256 * 3)?;
                    return Ok(Interruption::SetPalette { palette });
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_display => {
                    let width = self.instance.reg(Reg::A0);
                    let height = self.instance.reg(Reg::A1);
                    let address = self.instance.reg(Reg::A2);
                    log::trace!("Display called: {}x{}, 0x{:x}", width, height, address);
                    let pixels = width
                        .checked_mul(height)
                        .ok_or_else(|| "frame dimensions overflow".to_owned())?;
                    if pixels == 0 || pixels > (crate::MAX_FRAME_BYTES / 4) as u64 {
                        return Err("frame dimensions exceed the host limit".into());
                    }
                    let address = u32::try_from(address)
                        .map_err(|_| "frame address is out of range".to_owned())?;
                    let framebuffer = self.instance.read_memory(address, pixels as u32)?;
                    return Ok(Interruption::Display {
                        width,
                        height,
                        framebuffer,
                    });
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_epoca_inputs => {
                    let address = u32::try_from(self.instance.reg(Reg::A0))
                        .map_err(|_| "input address is out of range".to_owned())?;
                    let capacity =
                        usize::try_from(self.instance.reg(Reg::A1)).unwrap_or(usize::MAX);
                    let event_count =
                        (capacity / crate::INPUT_EVENT_BYTES).min(self.epoca_input_events.len());
                    let mut written = 0usize;
                    for _ in 0..event_count {
                        let Some(event) = self.epoca_input_events.pop_front() else {
                            break;
                        };
                        let destination = address
                            .checked_add(written as u32)
                            .ok_or_else(|| "input destination overflow".to_owned())?;
                        self.instance.write_memory(destination, &event)?;
                        written += event.len();
                    }
                    self.instance.set_reg(Reg::A0, written as u64);
                    continue;
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_asset_read => {
                    let name_address = u32::try_from(self.instance.reg(Reg::A0))
                        .map_err(|_| "asset name address is out of range".to_owned())?;
                    let name_length = usize::try_from(self.instance.reg(Reg::A1))
                        .unwrap_or(usize::MAX)
                        .min(crate::MAX_ASSET_NAME_BYTES);
                    let offset = usize::try_from(self.instance.reg(Reg::A2)).unwrap_or(usize::MAX);
                    let destination = u32::try_from(self.instance.reg(Reg::A3))
                        .map_err(|_| "asset destination is out of range".to_owned())?;
                    let capacity = usize::try_from(self.instance.reg(Reg::A4))
                        .unwrap_or(usize::MAX)
                        .min(crate::MAX_ASSET_READ_BYTES);
                    let name = self
                        .instance
                        .read_memory(name_address, name_length as u32)?;
                    let Some(file) = self.filesystem.get(&name) else {
                        self.instance.set_reg(Reg::A0, 0);
                        continue;
                    };
                    let length = capacity.min(file.blob.len().saturating_sub(offset));
                    if length == 0 {
                        self.instance.set_reg(Reg::A0, 0);
                        continue;
                    }
                    self.instance
                        .write_memory(destination, &file.blob[offset..offset + length])?;
                    self.instance.set_reg(Reg::A0, length as u64);
                    continue;
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_time_ms => {
                    self.instance
                        .set_reg(Reg::A0, self.started.elapsed().as_millis() as u64);
                    continue;
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_log => {
                    let address = u32::try_from(self.instance.reg(Reg::A0))
                        .map_err(|_| "log address is out of range".to_owned())?;
                    let length =
                        u32::try_from(self.instance.reg(Reg::A1).min(crate::MAX_LOG_BYTES as u64))
                            .map_err(|_| "log length is out of range".to_owned())?;
                    let message = self.instance.read_memory(address, length)?;
                    log::info!(target: "epoca_pvm_guest", "{}", String::from_utf8_lossy(&message));
                    continue;
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_yield => {
                    return Ok(Interruption::Yield);
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_epoca_audio => {
                    let address = u32::try_from(self.instance.reg(Reg::A0))
                        .map_err(|_| "audio address is out of range".to_owned())?;
                    let sample_count =
                        usize::try_from(self.instance.reg(Reg::A1)).unwrap_or(usize::MAX);
                    if sample_count == 0
                        || sample_count % crate::AUDIO_CHANNELS as usize != 0
                        || sample_count > crate::MAX_AUDIO_SAMPLES_PER_CALL
                    {
                        self.instance.set_reg(Reg::A0, 1);
                        continue;
                    }
                    let mut buffer = vec![0i16; sample_count];
                    self.instance.read_memory_into(address, unsafe {
                        core::slice::from_raw_parts_mut(
                            buffer.as_mut_ptr().cast::<u8>(),
                            sample_count * core::mem::size_of::<i16>(),
                        )
                    })?;
                    self.instance.set_reg(Reg::A0, 0);
                    return Ok(Interruption::AudioFrame { buffer });
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_fetch_inputs => {
                    let address = self.instance.reg(Reg::A0);
                    let requested =
                        usize::try_from(self.instance.reg(Reg::A1)).unwrap_or(usize::MAX);
                    let (first, second) = queued_input_chunks(&self.input_events, requested);
                    let mut written = 0usize;

                    for events in [first, second] {
                        if events.is_empty() {
                            continue;
                        }
                        let address = input_destination(address, written)?;
                        self.instance.write_memory(address, unsafe {
                            core::slice::from_raw_parts(
                                events.as_ptr().cast::<u8>(),
                                core::mem::size_of_val(events),
                            )
                        })?;
                        written += events.len();
                    }

                    for _ in 0..written {
                        self.input_events.pop_front();
                    }
                    self.instance.set_reg(Reg::A0, written as u64);
                    continue;
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_init_audio => {
                    let channels = self.instance.reg(Reg::A0) as u32;
                    let bits_per_sample = self.instance.reg(Reg::A1);
                    let sample_rate = self.instance.reg(Reg::A2) as u32;
                    if bits_per_sample != 16 {
                        self.instance.set_reg(Reg::A0, 0);
                        continue;
                    }

                    self.audio_channels = channels;
                    self.instance.set_reg(Reg::A0, 1);
                    return Ok(Interruption::AudioInit {
                        channels,
                        sample_rate,
                    });
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_output_audio => {
                    let address = self.instance.reg(Reg::A0);
                    let samples = self.instance.reg(Reg::A1) as usize;
                    let channels = self.audio_channels as usize;
                    let length = samples.saturating_mul(channels).min(1024 * 64);
                    let address = u32::try_from(address)
                        .map_err(|_| "audio address is out of range".to_owned())?;
                    let mut buffer: Vec<i16> = Vec::with_capacity(length);
                    unsafe {
                        self.instance.read_memory_into(
                            address,
                            core::slice::from_raw_parts_mut(
                                buffer
                                    .spare_capacity_mut()
                                    .as_mut_ptr()
                                    .cast::<MaybeUninit<u8>>(),
                                length * core::mem::size_of::<i16>(),
                            ),
                        )?;
                        buffer.set_len(length);
                    }

                    return Ok(Interruption::AudioFrame { buffer });
                }
                InterruptKind::Ecalli(hostcall) if Some(hostcall) == self.import_syscall => {
                    let syscall = self.instance.reg(Reg::A0);
                    let a1 = self.instance.reg(Reg::A1);
                    let a2 = self.instance.reg(Reg::A2);
                    let a3 = self.instance.reg(Reg::A3);
                    let a4 = self.instance.reg(Reg::A4);
                    let a5 = self.instance.reg(Reg::A5);
                    let pc = self.instance.program_counter();
                    log::trace!(
                        "Syscall at pc={pc:?}: {syscall:>3}, args = [0x{a1:>016x}, 0x{a2:>016x}, 0x{a3:>016x}, 0x{a4:>016x}, 0x{a5:>016x}]"
                    );

                    match syscall {
                        SYS_read => {
                            let result = self.handle_read(a1, a2, a3)?;
                            self.instance.set_reg(Reg::A0, result);
                            continue;
                        }
                        SYS_readv => {
                            if a3 == 0 || a3 > IOV_MAX {
                                self.instance.set_reg(Reg::A0, errno(EINVAL));
                                continue;
                            }

                            let mut total_length = 0u64;
                            for n in 0..a3 {
                                let address =
                                    self.instance.read_u64(a2.wrapping_add(n * 16) as u32)?;
                                let length = self
                                    .instance
                                    .read_u64(a2.wrapping_add(n * 16).wrapping_add(8) as u32)?;
                                let bytes_read = self.handle_read(a1, address, length)?;
                                if (bytes_read as i64) < 0 {
                                    self.instance.set_reg(Reg::A0, bytes_read);
                                    continue 'outer_loop;
                                }

                                total_length = total_length
                                    .checked_add(bytes_read)
                                    .ok_or_else(|| "readv byte count overflow".to_owned())?;
                                if bytes_read < length {
                                    break;
                                }
                            }

                            self.instance.set_reg(Reg::A0, total_length);
                            continue;
                        }
                        SYS_writev => {
                            if a3 == 0 || a3 > IOV_MAX {
                                self.instance.set_reg(Reg::A0, errno(EINVAL));
                                continue;
                            }

                            let mut total_length = 0u64;
                            for n in 0..a3 {
                                let address =
                                    self.instance.read_u64(a2.wrapping_add(n * 16) as u32)?;
                                let length = self
                                    .instance
                                    .read_u64(a2.wrapping_add(n * 16).wrapping_add(8) as u32)?;
                                let bytes_written = self.handle_write(a1, address, length)?;
                                if (bytes_written as i64) < 0 {
                                    self.instance.set_reg(Reg::A0, bytes_written);
                                    continue 'outer_loop;
                                }

                                total_length = total_length
                                    .checked_add(bytes_written)
                                    .ok_or_else(|| "writev byte count overflow".to_owned())?;
                                if bytes_written < length {
                                    break;
                                }
                            }

                            self.instance.set_reg(Reg::A0, total_length);
                            continue;
                        }
                        SYS_exit => {
                            log::info!("Exit called: status={}", a1);
                            if a1 == 0 {
                                return Ok(Interruption::Exit);
                            } else {
                                return Err(format!("exit called with status: {a1}"));
                            }
                        }
                        SYS_openat => {
                            if a1 == AT_FDCWD {
                                let Some(path) = self.read_cstr(a2)? else {
                                    self.instance.set_reg(Reg::A0, errno(EFAULT));
                                    continue;
                                };

                                let result = self.handle_open(&path, a3);
                                self.instance.set_reg(Reg::A0, result);
                                continue;
                            }
                        }
                        SYS_lseek => {
                            let result = self.handle_lseek(a1, a2 as i64, a3);
                            self.instance.set_reg(Reg::A0, result);
                            continue;
                        }
                        SYS_close => {
                            let result = self.handle_close(a1);
                            self.instance.set_reg(Reg::A0, result);
                            continue;
                        }
                        _ => {
                            log::debug!("Unimplemented syscall at pc={pc:?}: {syscall:>3}, args = [0x{a1:>016x}, 0x{a2:>016x}, 0x{a3:>016x}, 0x{a4:>016x}, 0x{a5:>016x}]");
                        }
                    }

                    self.instance.set_reg(Reg::A0, errno(ENOSYS));
                }
                InterruptKind::Finished => {
                    return Ok(Interruption::Exit);
                }
                InterruptKind::Ecalli(hostcall) => {
                    return Err(format!("unsupported host call: {hostcall}"));
                }
                InterruptKind::Trap => {
                    return Err(format!(
                        "execution trapped at {:?}",
                        self.instance.program_counter()
                    ));
                }
                InterruptKind::NotEnoughGas => {
                    return Err("ran out of gas".into());
                }
                InterruptKind::Segfault(address) => {
                    return Err(format!("guest segfault at {address:?}"));
                }
                InterruptKind::Step => return Err("unexpected guest step".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests;
