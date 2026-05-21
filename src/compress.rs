use std::io::{Read, Write};
use anyhow::Result;

/// Default ZSTD compression level (balanced ratio/speed)
pub const DEFAULT_LEVEL: i32 = 19;

/// Default window log: 256 MiB = 2^28
pub const DEFAULT_WINDOW_LOG: u32 = 28;

/// Max-mode window log: 1 GiB = 2^30 (2^31=2 GiB may exceed system limits)
pub const MAX_WINDOW_LOG: u32 = 30;

/// Eco-mode window log: 128 MiB = 2^27
pub const ECO_WINDOW_LOG: u32 = 27;

/// Compression parameters for ZSTD.
#[derive(Debug, Clone)]
pub struct CompressParams {
    pub level: i32,
    pub window_log: u32,
    pub threads: u32,
    pub ldm: bool,
}

impl Default for CompressParams {
    fn default() -> Self {
        Self {
            level: DEFAULT_LEVEL,
            window_log: DEFAULT_WINDOW_LOG,
            threads: 2,
            ldm: true,
        }
    }
}

impl CompressParams {
    /// Max mode: --ultra level 22, large window, all cores, no memory cap.
    pub fn max() -> Self {
        Self {
            level: 22,
            window_log: MAX_WINDOW_LOG,
            threads: 0, // 0 = auto-detect all cores
            ldm: true,
        }
    }

    /// Eco mode tuned for a given memory budget (in MB).
    /// Uses lower level and smaller window to stay within budget.
    pub fn eco(mem_mb: u32) -> Self {
        // Window log 27 = 128 MiB, 26 = 64 MiB, etc.
        let window_log = if mem_mb >= 256 {
            ECO_WINDOW_LOG
        } else if mem_mb >= 128 {
            26
        } else {
            25
        };
        Self {
            level: 15,
            window_log,
            threads: 2,
            ldm: true,
        }
    }
}

/// Create a ZSTD-compressing writer from an inner writer and params.
///
/// The returned `Box<dyn Write>` transparently compresses data written to it.
/// The caller **must** call `finish_compressor()` or drop the inner encoder
/// properly to finalize the ZSTD frame.
pub fn create_compressor<W: Write + 'static>(
    inner: W,
    params: &CompressParams,
) -> Result<Box<dyn Write>> {
    let mut enc = zstd::stream::write::Encoder::new(inner, params.level)?;

    if params.window_log != DEFAULT_WINDOW_LOG {
        enc.window_log(params.window_log)?;
    }

    if params.ldm {
        enc.long_distance_matching(true)?;
    }

    if params.threads > 0 {
        enc.multithread(params.threads)?;
    }

    // auto_finish ensures the ZSTD frame is finalized on drop
    Ok(Box::new(enc.auto_finish()))
}

/// Create a ZSTD-decompressing reader from an inner reader.
///
/// Sets `window_log_max` to MAX_WINDOW_LOG so archives compressed with
/// large windows (e.g. `--max`) can be decompressed without hitting the
/// default safe memory limit.
pub fn create_decompressor<R: Read + 'static>(
    inner: R,
) -> Result<Box<dyn Read>> {
    let mut dec = zstd::stream::read::Decoder::new(inner)?;
    // Allow up to 2 GiB window (matches MAX_WINDOW_LOG from --max).
    // The decoder's own safety limit is more restrictive by default.
    dec.window_log_max(31)?;
    Ok(Box::new(dec))
}
