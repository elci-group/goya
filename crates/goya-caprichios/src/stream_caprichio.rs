//! A bounded process-local byte stream construction recipe and safe runtime.
//!
//! Writes and reads are nonblocking, chunk-atomic operations. The stream is a
//! mutex-protected FIFO within one Rust process; no network, cross-process,
//! operating-system pipe, durability, or scheduler behavior is claimed.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

pub const CONTRACT: &str = "goya.data.stream.process_local_bounded.v1";
pub const RECIPE_VERSION: &str = "0.1.0";
pub const MAX_BUFFERED_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_CHUNKS: u32 = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamDeclaration {
    /// Upper bound on queued payload bytes; queue metadata is bounded separately.
    pub max_buffered_bytes: u64,
    /// Upper bound on queued chunks, preventing tiny chunks from causing
    /// unbounded per-entry allocation overhead.
    pub max_chunks: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamIr {
    max_buffered_bytes: u64,
    max_chunks: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    CrossProcess,
    Browser,
    Node,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub process_local_memory: bool,
    pub standard_mutex: bool,
    pub pointer_width_bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroByteCapacity,
    ZeroChunkCapacity,
    ByteCapacityTooLarge,
    ChunkCapacityTooLarge,
    UnsupportedTarget,
    UnsupportedPointerWidth,
    CapacityDoesNotFitTarget,
    MissingProcessLocalMemory,
    MissingStandardMutex,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroByteCapacity => "maximum buffered byte capacity must be positive",
            Self::ZeroChunkCapacity => "maximum queued chunk count must be positive",
            Self::ByteCapacityTooLarge => "maximum buffered bytes exceed the supported bound",
            Self::ChunkCapacityTooLarge => "maximum queued chunks exceed the supported bound",
            Self::UnsupportedTarget => "stream factory supports only process-local Rust",
            Self::UnsupportedPointerWidth => "target pointer width must be 16, 32, or 64 bits",
            Self::CapacityDoesNotFitTarget => {
                "stream capacities do not fit the target pointer width"
            }
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
            Self::MissingStandardMutex => "target runtime must provide a standard mutex",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamError {
    ZeroCapacity,
    ZeroChunkLimit,
    CapacityTooLarge,
    ChunkLimitTooLarge,
    ChunkTooLarge,
    FullBytes,
    FullChunks,
    AllocationFailed,
    Closed,
    Poisoned,
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroCapacity => "stream byte capacity must be positive",
            Self::ZeroChunkLimit => "stream chunk capacity must be positive",
            Self::CapacityTooLarge => "stream byte capacity exceeds the supported bound",
            Self::ChunkLimitTooLarge => "stream chunk limit exceeds the supported bound",
            Self::ChunkTooLarge => "chunk exceeds the stream byte capacity",
            Self::FullBytes => "stream has insufficient remaining byte capacity",
            Self::FullChunks => "stream has reached its queued chunk limit",
            Self::AllocationFailed => "stream could not reserve memory for the chunk",
            Self::Closed => "stream is closed for writes",
            Self::Poisoned => "stream lock was poisoned",
        })
    }
}

impl std::error::Error for StreamError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadOutcome {
    Chunk(Vec<u8>),
    /// No chunk is currently queued and the stream remains open.
    Empty,
    /// The stream is closed and all previously queued chunks have been read.
    EndOfStream,
}

struct StreamState {
    chunks: VecDeque<Vec<u8>>,
    buffered_bytes: usize,
    closed: bool,
}

/// Safe in-memory byte stream with nonblocking, atomic chunk admission.
/// `try_write` copies a chunk only after all capacity checks succeed. Buffered
/// payload bytes and chunk count both stay within their configured limits.
pub struct BoundedByteStream {
    max_buffered_bytes: usize,
    max_chunks: usize,
    state: Mutex<StreamState>,
}

impl BoundedByteStream {
    pub fn new(max_buffered_bytes: usize, max_chunks: usize) -> Result<Self, StreamError> {
        if max_buffered_bytes == 0 {
            return Err(StreamError::ZeroCapacity);
        }
        if max_chunks == 0 {
            return Err(StreamError::ZeroChunkLimit);
        }
        if max_buffered_bytes as u64 > MAX_BUFFERED_BYTES {
            return Err(StreamError::CapacityTooLarge);
        }
        if max_chunks as u64 > MAX_CHUNKS as u64 {
            return Err(StreamError::ChunkLimitTooLarge);
        }
        Ok(Self {
            max_buffered_bytes,
            max_chunks,
            state: Mutex::new(StreamState {
                chunks: VecDeque::new(),
                buffered_bytes: 0,
                closed: false,
            }),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, StreamState>, StreamError> {
        self.state.lock().map_err(|_| StreamError::Poisoned)
    }

    pub fn try_write(&self, chunk: &[u8]) -> Result<(), StreamError> {
        let mut state = self.lock()?;
        if state.closed {
            return Err(StreamError::Closed);
        }
        if chunk.is_empty() {
            return Ok(());
        }
        if chunk.len() > self.max_buffered_bytes {
            return Err(StreamError::ChunkTooLarge);
        }
        if state.chunks.len() >= self.max_chunks {
            return Err(StreamError::FullChunks);
        }
        if chunk.len() > self.max_buffered_bytes - state.buffered_bytes {
            return Err(StreamError::FullBytes);
        }
        let mut owned_chunk = Vec::new();
        owned_chunk
            .try_reserve_exact(chunk.len())
            .map_err(|_| StreamError::AllocationFailed)?;
        owned_chunk.extend_from_slice(chunk);
        state
            .chunks
            .try_reserve(1)
            .map_err(|_| StreamError::AllocationFailed)?;
        state.chunks.push_back(owned_chunk);
        state.buffered_bytes += chunk.len();
        Ok(())
    }

    pub fn try_read(&self) -> Result<ReadOutcome, StreamError> {
        let mut state = self.lock()?;
        if let Some(chunk) = state.chunks.pop_front() {
            state.buffered_bytes -= chunk.len();
            return Ok(ReadOutcome::Chunk(chunk));
        }
        if state.closed {
            Ok(ReadOutcome::EndOfStream)
        } else {
            Ok(ReadOutcome::Empty)
        }
    }

    /// Close for subsequent writes while leaving buffered chunks readable.
    /// Repeated closes are harmless.
    pub fn close(&self) -> Result<(), StreamError> {
        self.lock()?.closed = true;
        Ok(())
    }

    pub fn buffered_bytes(&self) -> Result<usize, StreamError> {
        Ok(self.lock()?.buffered_bytes)
    }

    pub fn queued_chunks(&self) -> Result<usize, StreamError> {
        Ok(self.lock()?.chunks.len())
    }

    pub fn max_buffered_bytes(&self) -> usize {
        self.max_buffered_bytes
    }

    pub fn max_chunks(&self) -> usize {
        self.max_chunks
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl StreamDeclaration {
    pub fn normalize(self) -> Result<StreamIr, Diagnostic> {
        if self.max_buffered_bytes == 0 {
            return Err(Diagnostic::ZeroByteCapacity);
        }
        if self.max_chunks == 0 {
            return Err(Diagnostic::ZeroChunkCapacity);
        }
        if self.max_buffered_bytes > MAX_BUFFERED_BYTES {
            return Err(Diagnostic::ByteCapacityTooLarge);
        }
        if self.max_chunks > MAX_CHUNKS {
            return Err(Diagnostic::ChunkCapacityTooLarge);
        }
        Ok(StreamIr {
            max_buffered_bytes: self.max_buffered_bytes,
            max_chunks: self.max_chunks,
        })
    }
}

impl StreamIr {
    pub fn max_buffered_bytes(&self) -> u64 {
        self.max_buffered_bytes
    }

    pub fn max_chunks(&self) -> u32 {
        self.max_chunks
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nstream=bounded_fifo_bytes\nmax_buffered_bytes={}\nmax_chunks={}\nwrite=nonblocking_atomic_chunk\nread=nonblocking_chunk_or_empty_or_eof\nclose=drain_then_eof\n",
            self.max_buffered_bytes, self.max_chunks
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<StreamArtifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(capabilities.pointer_width_bits, 16 | 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        let target_max = (1u128 << capabilities.pointer_width_bits) - 1;
        if self.max_buffered_bytes as u128 > target_max || self.max_chunks as u128 > target_max {
            return Err(Diagnostic::CapacityDoesNotFitTarget);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        if !capabilities.standard_mutex {
            return Err(Diagnostic::MissingStandardMutex);
        }
        Ok(StreamArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; process-local nonblocking bounded chunk stream.\npub fn make_byte_stream() -> Result<crate::stream_caprichio::BoundedByteStream, crate::stream_caprichio::StreamError> {{\n    crate::stream_caprichio::BoundedByteStream::new({}usize, {}usize)\n}}\n",
                self.max_buffered_bytes, self.max_chunks
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
        standard_mutex: true,
        pointer_width_bits: 64,
    };

    fn declaration() -> StreamDeclaration {
        StreamDeclaration {
            max_buffered_bytes: 16,
            max_chunks: 4,
        }
    }

    #[test]
    fn stream_has_bounded_atomic_fifo_and_distinct_empty_eof_semantics() {
        let stream = BoundedByteStream::new(7, 2).unwrap();
        assert_eq!(stream.try_read().unwrap(), ReadOutcome::Empty);
        stream.try_write(&[1, 2, 3]).unwrap();
        stream.try_write(&[4, 5]).unwrap();
        assert_eq!(stream.buffered_bytes().unwrap(), 5);
        assert_eq!(stream.queued_chunks().unwrap(), 2);
        assert_eq!(stream.try_write(&[6]), Err(StreamError::FullChunks));
        assert_eq!(
            stream.try_read().unwrap(),
            ReadOutcome::Chunk(vec![1, 2, 3])
        );
        stream.try_write(&[6, 7, 8, 9]).unwrap();
        assert_eq!(stream.buffered_bytes().unwrap(), 6);
        assert_eq!(stream.try_read().unwrap(), ReadOutcome::Chunk(vec![4, 5]));
        assert_eq!(
            stream.try_read().unwrap(),
            ReadOutcome::Chunk(vec![6, 7, 8, 9])
        );
        assert_eq!(stream.buffered_bytes().unwrap(), 0);
        assert_eq!(stream.try_read().unwrap(), ReadOutcome::Empty);
        stream.close().unwrap();
        assert_eq!(stream.try_read().unwrap(), ReadOutcome::EndOfStream);
        assert_eq!(stream.try_write(&[10]), Err(StreamError::Closed));
        assert_eq!(stream.try_write(&[]), Err(StreamError::Closed));
        stream.close().unwrap();
    }

    #[test]
    fn rejects_zero_limits_oversized_chunks_and_byte_overflow_atomically() {
        assert_eq!(
            BoundedByteStream::new(0, 1).err(),
            Some(StreamError::ZeroCapacity)
        );
        assert_eq!(
            BoundedByteStream::new(1, 0).err(),
            Some(StreamError::ZeroChunkLimit)
        );
        assert_eq!(
            BoundedByteStream::new(MAX_BUFFERED_BYTES as usize + 1, 1).err(),
            Some(StreamError::CapacityTooLarge)
        );
        assert_eq!(
            BoundedByteStream::new(1, MAX_CHUNKS as usize + 1).err(),
            Some(StreamError::ChunkLimitTooLarge)
        );
        let stream = BoundedByteStream::new(4, 3).unwrap();
        assert_eq!(
            stream.try_write(&[1, 2, 3, 4, 5]),
            Err(StreamError::ChunkTooLarge)
        );
        stream.try_write(&[1, 2, 3]).unwrap();
        assert_eq!(stream.try_write(&[4, 5]), Err(StreamError::FullBytes));
        assert_eq!(stream.buffered_bytes().unwrap(), 3);
        assert_eq!(stream.queued_chunks().unwrap(), 1);
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let ir = declaration().normalize().unwrap();
        let first = ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap();
        let second = ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("max_buffered_bytes=16\n"));
        assert!(first.canonical_ir.contains("max_chunks=4\n"));
        assert!(first
            .source
            .contains("BoundedByteStream::new(16usize, 4usize)"));
    }

    #[test]
    fn declaration_and_negotiation_fail_closed() {
        assert_eq!(
            StreamDeclaration {
                max_buffered_bytes: 0,
                max_chunks: 1,
            }
            .normalize(),
            Err(Diagnostic::ZeroByteCapacity)
        );
        assert_eq!(
            StreamDeclaration {
                max_buffered_bytes: 1,
                max_chunks: 0,
            }
            .normalize(),
            Err(Diagnostic::ZeroChunkCapacity)
        );
        assert_eq!(
            StreamDeclaration {
                max_buffered_bytes: MAX_BUFFERED_BYTES + 1,
                max_chunks: 1,
            }
            .normalize(),
            Err(Diagnostic::ByteCapacityTooLarge)
        );
        assert_eq!(
            StreamDeclaration {
                max_buffered_bytes: 1,
                max_chunks: MAX_CHUNKS + 1,
            }
            .normalize(),
            Err(Diagnostic::ChunkCapacityTooLarge)
        );
        let ir = declaration().normalize().unwrap();
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, LOCAL),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            StreamDeclaration {
                max_buffered_bytes: 65_536,
                max_chunks: 4,
            }
            .normalize()
            .unwrap()
            .lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    pointer_width_bits: 16,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::CapacityDoesNotFitTarget)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    process_local_memory: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingProcessLocalMemory)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    standard_mutex: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingStandardMutex)
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_with_runtime_contract() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, LOCAL)
            .unwrap();
        let runtime_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/stream_caprichio.rs");
        let harness = format!(
            "#[path = {runtime_path:?}]\npub mod stream_caprichio;\n{}\nfn main() {{\n    let stream = make_byte_stream().unwrap();\n    assert_eq!(stream.max_buffered_bytes(), 16);\n    assert_eq!(stream.max_chunks(), 4);\n    stream.try_write(&[7, 8]).unwrap();\n    assert_eq!(stream.try_read().unwrap(), stream_caprichio::ReadOutcome::Chunk(vec![7, 8]));\n    stream.close().unwrap();\n    assert_eq!(stream.try_read().unwrap(), stream_caprichio::ReadOutcome::EndOfStream);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output = std::env::temp_dir().join(format!(
            "goya-generated-stream-factory-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(output.clone());
        let mut rustc = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&output)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        rustc
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let compile = rustc.wait_with_output().unwrap();
        assert!(
            compile.status.success(),
            "generated source failed to compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&output).output().unwrap();
        assert!(
            run.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    #[test]
    #[ignore = "construction microbenchmark"]
    fn benchmark_stream_factory_construction() {
        let ir = declaration().normalize().unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap());
        }
        println!(
            "100,000 stream factory constructions in {:?}",
            start.elapsed()
        );
    }

    #[test]
    #[ignore = "runtime microbenchmark"]
    fn benchmark_stream_runtime_round_trips() {
        let stream = BoundedByteStream::new(4096, 4).unwrap();
        let chunk = [0x5a; 1024];
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            stream.try_write(&chunk).unwrap();
            std::hint::black_box(stream.try_read().unwrap());
        }
        println!("100,000 1KiB stream round trips in {:?}", start.elapsed());
    }
}

#[cfg(test)]
struct BinaryCleanup(std::path::PathBuf);

#[cfg(test)]
impl Drop for BinaryCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
