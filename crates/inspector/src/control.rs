use core::fmt::Debug;

/// Runtime controls for bounded/interruptible execution.
#[derive(Clone, Debug, Default)]
pub struct ExecutionControl {
    /// Maximum opcode steps allowed in this run segment.
    ///
    /// `None` means unbounded.
    pub max_steps: Option<u64>,
}

/// Controlled execution output that can either complete or suspend.
#[derive(Clone, Debug)]
pub enum ControlledExecutionResult<R, S> {
    /// Execution reached a terminal EVM result.
    Completed(R),
    /// Execution was suspended and returned resumable state.
    Suspended(SuspendedExecution<S>),
}

/// Suspended execution payload.
#[derive(Clone, Debug)]
pub struct SuspendedExecution<S> {
    /// Serialized or in-memory snapshot payload.
    pub snapshot: S,
    /// Configured step limit that triggered suspension.
    pub step_limit: u64,
    /// Number of opcode steps executed in this run invocation.
    pub steps_executed: u64,
    /// Cumulative steps across prior resumed invocations.
    pub total_steps_executed: u64,
}

/// In-memory resumable snapshot containing a full EVM instance.
#[derive(Clone, Debug)]
pub struct InMemoryExecutionSnapshot<EVM> {
    pub evm: EVM,
    pub total_steps_executed: u64,
}

/// Versioned snapshot envelope for forward-compatible persistence.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SnapshotEnvelope<S> {
    /// Snapshot schema version.
    pub version: u16,
    /// Snapshot payload.
    pub payload: S,
}

impl<S> SnapshotEnvelope<S> {
    /// Constructs a new snapshot envelope.
    #[inline]
    pub const fn new(version: u16, payload: S) -> Self {
        Self { version, payload }
    }
}

/// Codec abstraction to encode/decode snapshots independent of storage backend.
pub trait ExecutionSnapshotCodec<S> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn encode(&self, snapshot: &SnapshotEnvelope<S>) -> Result<Vec<u8>, Self::Error>;
    fn decode(&self, bytes: &[u8]) -> Result<SnapshotEnvelope<S>, Self::Error>;
}

/// Storage abstraction to persist snapshots in DB/memory/etc.
pub trait ExecutionSnapshotStore {
    type Error: std::error::Error + Send + Sync + 'static;

    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    fn delete(&mut self, key: &str) -> Result<(), Self::Error>;
}
