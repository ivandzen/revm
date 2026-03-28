//! Inspector is a crate that provides a set of traits that allow inspecting the EVM execution.
//!
//! It is used to implement tracers that can be used to inspect the EVM execution.
//! Implementing inspection is optional and it does not effect the core execution.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

mod control;
mod controlled;
mod count_inspector;
#[cfg(feature = "tracer")]
mod eip3155;
mod either;
mod gas;
/// Handler implementations for inspector integration.
pub mod handler;
mod inspect;
mod inspector;
mod mainnet_inspect;
mod noop;
/// Test inspector for testing EVM execution.
pub mod test_inspector;
mod traits;

#[cfg(test)]
mod inspector_tests;

/// Inspector implementations.
pub mod inspectors {
    #[cfg(feature = "tracer")]
    pub use super::eip3155::TracerEip3155;
    pub use super::gas::GasInspector;
}

pub use context;
pub use database_interface;
pub use handler as evm_handler;
pub use interpreter;
pub use primitives;
pub use state;

pub use control::*;
pub use controlled::ControlledInspector;
pub use count_inspector::CountInspector;
pub use handler::{inspect_instructions, InspectorHandler};
pub use inspect::{InspectCommitEvm, InspectControlledEvm, InspectEvm, InspectSystemCallEvm};
pub use inspector::*;
pub use noop::NoOpInspector;
pub use test_inspector::{InspectorEvent, InterpreterState, StepRecord, TestInspector};
pub use traits::*;

#[cfg(test)]
mod tests {
    use super::*;
    use context::{BlockEnv, CfgEnv, Context, Journal, TxEnv};
    use database::{BenchmarkDB, BENCH_CALLER, BENCH_TARGET};
    use ::handler::{MainBuilder, MainContext};
    use interpreter::{interpreter::EthInterpreter, InstructionResult, InterpreterTypes};
    use primitives::TxKind;
    use state::{bytecode::opcode, Bytecode};

    struct HaltInspector;
    impl<CTX, INTR: InterpreterTypes> Inspector<CTX, INTR> for HaltInspector {
        fn step(&mut self, interp: &mut interpreter::Interpreter<INTR>, _context: &mut CTX) {
            interp.halt(InstructionResult::Stop);
        }
    }

    #[test]
    fn test_step_halt() {
        let bytecode = [opcode::INVALID];
        let r = run(&bytecode, HaltInspector);
        assert!(r.is_success());
    }

    #[test]
    fn inspect_one_tx_controlled_completes_when_unbounded() {
        let bytecode = Bytecode::new_raw([opcode::STOP].to_vec().into());
        let ctx = Context::mainnet().with_db(BenchmarkDB::new_bytecode(bytecode));
        let mut evm = ctx.build_mainnet_with_inspector(NoOpInspector);

        let tx = TxEnv::builder()
            .caller(BENCH_CALLER)
            .kind(TxKind::Call(BENCH_TARGET))
            .gas_limit(21100)
            .build()
            .unwrap();

        let output = evm
            .inspect_one_tx_controlled(tx, &ExecutionControl { max_steps: None })
            .unwrap();

        match output {
            ControlledExecutionResult::Completed(result) => assert!(result.is_success()),
            ControlledExecutionResult::Suspended(_) => {
                panic!("expected completed execution for unbounded control")
            }
        }
    }

    #[test]
    fn inspect_one_tx_controlled_suspends_and_resumes() {
        let bytecode = Bytecode::new_raw([opcode::PUSH0, opcode::STOP].to_vec().into());
        let ctx = Context::mainnet().with_db(BenchmarkDB::new_bytecode(bytecode));
        let mut evm = ctx.build_mainnet_with_inspector(NoOpInspector);

        let tx = TxEnv::builder()
            .caller(BENCH_CALLER)
            .kind(TxKind::Call(BENCH_TARGET))
            .gas_limit(21100)
            .build()
            .unwrap();

        let suspended = evm
            .inspect_one_tx_controlled(tx.clone(), &ExecutionControl { max_steps: Some(1) })
            .unwrap();

        let mut snapshot = match suspended {
            ControlledExecutionResult::Completed(_) => {
                panic!("expected suspended execution for step-limited control")
            }
            ControlledExecutionResult::Suspended(suspended) => {
                assert_eq!(suspended.step_limit, 1);
                assert_eq!(suspended.steps_executed, 1);
                assert_eq!(suspended.total_steps_executed, 1);
                suspended.snapshot
            }
        };

        let resumed = snapshot
            .evm
            .inspect_one_tx_controlled(
                TxEnv::builder()
                    .caller(BENCH_CALLER)
                    .kind(TxKind::Call(BENCH_TARGET))
                    .nonce(1)
                    .gas_limit(21100)
                    .build()
                    .unwrap(),
                &ExecutionControl { max_steps: None },
            )
            .unwrap();
        match resumed {
            ControlledExecutionResult::Completed(result) => assert!(result.is_success()),
            ControlledExecutionResult::Suspended(_) => {
                panic!("expected resumed execution to complete")
            }
        }
    }

    fn run(
        bytecode: &[u8],
        inspector: impl Inspector<
            Context<BlockEnv, TxEnv, CfgEnv, BenchmarkDB, Journal<BenchmarkDB>, ()>,
            EthInterpreter,
        >,
    ) -> context::result::ExecutionResult {
        let bytecode = Bytecode::new_raw(bytecode.to_vec().into());
        let ctx = Context::mainnet().with_db(BenchmarkDB::new_bytecode(bytecode));
        let mut evm = ctx.build_mainnet_with_inspector(inspector);
        evm.inspect_one_tx(
            TxEnv::builder()
                .caller(BENCH_CALLER)
                .kind(TxKind::Call(BENCH_TARGET))
                .gas_limit(21100)
                .build()
                .unwrap(),
        )
        .unwrap()
    }
}
