//! Shared by every Rust engine harness: the corpus format, the render-step
//! test, percentiles and the process memory footprint.

/// One inbound frame and the frames the worker must answer it with.
pub struct Step {
    pub inbound: Vec<u8>,
    pub outbound: Vec<Vec<u8>>,
}

/// Read `corpus.bin` as written by `workloads/src/corpus-format.ts`.
pub fn load_corpus(path: &str) -> Vec<Step> {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let mut offset = 0;
    let mut take = |len: usize| {
        let slice = bytes[offset..offset + len].to_vec();
        offset += len;
        slice
    };
    assert_eq!(take(4), b"QJSC", "not a QJSC corpus");
    let read_u32 = |bytes: Vec<u8>| u32::from_le_bytes(bytes.try_into().unwrap()) as usize;
    assert_eq!(read_u32(take(4)), 1, "unsupported corpus version");
    let count = read_u32(take(4));
    (0..count)
        .map(|_| {
            let inbound_len = read_u32(take(4));
            let inbound = take(inbound_len);
            let frames = read_u32(take(4));
            let outbound = (0..frames)
                .map(|_| {
                    let len = read_u32(take(4));
                    take(len)
                })
                .collect();
            Step { inbound, outbound }
        })
        .collect()
}

const RENDERER_TRAIT: u8 = 17;
const RENDERER_RENDER_METHOD: u8 = 0;
const MESSAGE_TYPE_START: u8 = 0;

/// Whether `frame` is the host starting a render (trait 17, method 0, Start).
pub fn is_render_start(frame: &[u8]) -> bool {
    let (id_len, prefix) = match frame.first() {
        Some(first) if first & 0b11 == 0 => ((first >> 2) as usize, 1),
        Some(_) if frame.len() > 1 => ((((frame[0] >> 2) as usize) | ((frame[1] as usize) << 6)) & 0x3fff, 2),
        _ => return false,
    };
    let header = prefix + id_len;
    frame.get(header..header + 3) == Some(&[RENDERER_TRAIT, RENDERER_RENDER_METHOD, MESSAGE_TYPE_START][..])
}

/// `(p50, p95)` of the samples, in the samples' unit.
pub fn percentiles(samples: &mut [f64]) -> (f64, f64) {
    samples.sort_by(f64::total_cmp);
    let at = |fraction: f64| samples[((samples.len() as f64 * fraction) as usize).min(samples.len() - 1)];
    (at(0.5), at(0.95))
}

/// This process's physical footprint in bytes: the number Xcode's memory
/// gauge and `footprint` report, and what jetsam limits.
pub fn footprint_bytes() -> u64 {
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let status = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            (&mut info as *mut libc::rusage_info_v2).cast::<libc::rusage_info_t>(),
        )
    };
    assert_eq!(status, 0, "proc_pid_rusage failed");
    info.ri_phys_footprint
}

pub fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
