use crate::{
    inspect::{InspectCommitEvm, InspectControlledEvm, InspectEvm, InspectSystemCallEvm},
    ControlledExecutionResult, ControlledInspector, InMemoryExecutionSnapshot, Inspector,
    InspectorEvmTr, InspectorHandler, JournalExt, SuspendedExecution,
};
use context::{ContextSetters, ContextTr, Evm, FrameStack, JournalTr};
use core::mem;
use database_interface::DatabaseCommit;
use handler::{
    instructions::InstructionProvider, system_call::SystemCallTx, EthFrame, EvmTr, EvmTrError,
    Handler, MainnetHandler, PrecompileProvider,
};
use interpreter::{interpreter::EthInterpreter, InterpreterResult};
use primitives::{Address, Bytes};
use state::EvmState;

// Implementing InspectorHandler for MainnetHandler.
impl<EVM, ERROR> InspectorHandler for MainnetHandler<EVM, ERROR, EthFrame<EthInterpreter>>
where
    EVM: InspectorEvmTr<
        Context: ContextTr<Journal: JournalTr<State = EvmState>>,
        Frame = EthFrame<EthInterpreter>,
        Inspector: Inspector<<<Self as Handler>::Evm as EvmTr>::Context, EthInterpreter>,
    >,
    ERROR: EvmTrError<EVM>,
{
    type IT = EthInterpreter;
}

// Implementing InspectEvm for Evm
impl<CTX, INSP, INST, PRECOMPILES> InspectEvm
    for Evm<CTX, INSP, INST, PRECOMPILES, EthFrame<EthInterpreter>>
where
    CTX: ContextSetters + ContextTr<Journal: JournalTr<State = EvmState> + JournalExt>,
    INSP: Inspector<CTX, EthInterpreter>,
    INST: InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    PRECOMPILES: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Inspector = INSP;

    fn set_inspector(&mut self, inspector: Self::Inspector) {
        self.inspector = inspector;
    }

    fn inspect_one_tx(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        self.set_tx(tx);
        MainnetHandler::default().inspect_run(self)
    }
}

impl<CTX, INSP, INST, PRECOMPILES> InspectControlledEvm
    for Evm<CTX, INSP, INST, PRECOMPILES, EthFrame<EthInterpreter>>
where
    CTX: Clone + ContextSetters + ContextTr<Journal: JournalTr<State = EvmState> + JournalExt>,
    INSP: Clone + Inspector<CTX, EthInterpreter>,
    INST: Clone + InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    PRECOMPILES: Clone + PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Snapshot = InMemoryExecutionSnapshot<Self>;

    fn inspect_one_tx_controlled(
        &mut self,
        tx: Self::Tx,
        control: &crate::ExecutionControl,
    ) -> Result<ControlledExecutionResult<Self::ExecutionResult, Self::Snapshot>, Self::Error> {
        let controlled_inspector =
            ControlledInspector::new(self.inspector.clone(), control.clone(), 0);
        let frame_stack = mem::take(&mut self.frame_stack);
        let mut controlled_evm = Evm {
            ctx: self.ctx.clone(),
            inspector: controlled_inspector,
            instruction: self.instruction.clone(),
            precompiles: self.precompiles.clone(),
            frame_stack,
        };
        controlled_evm.set_tx(tx);

        let mut h: MainnetHandler<_, Self::Error, EthFrame<EthInterpreter>> =
            MainnetHandler::default();
        let init_and_floor_gas = h.validate(&mut controlled_evm)?;
        let eip7702_refund = h.pre_execution(&mut controlled_evm)? as i64;

        let mut frame_result =
            match h.inspect_execution_controlled(&mut controlled_evm, &init_and_floor_gas)? {
                crate::handler::InspectExecOutcome::Completed(frame_result) => frame_result,
                crate::handler::InspectExecOutcome::Suspended => {
                    let (inner_inspector, steps_executed, total_steps_executed, step_limit) =
                        controlled_evm.inspector.into_parts();
                    let suspended_evm = Evm {
                        ctx: controlled_evm.ctx,
                        inspector: inner_inspector,
                        instruction: controlled_evm.instruction,
                        precompiles: controlled_evm.precompiles,
                        frame_stack: controlled_evm.frame_stack,
                    };
                    let step_limit = step_limit
                        .expect("controlled execution suspended without reason; this is a bug");
                    return Ok(ControlledExecutionResult::Suspended(SuspendedExecution {
                        snapshot: InMemoryExecutionSnapshot {
                            evm: suspended_evm,
                            total_steps_executed,
                        },
                        step_limit,
                        steps_executed,
                        total_steps_executed,
                    }));
                }
            };

        let result_gas = h.post_execution(
            &mut controlled_evm,
            &mut frame_result,
            init_and_floor_gas,
            eip7702_refund,
        )?;
        let result = h.execution_result(&mut controlled_evm, frame_result, result_gas)?;

        let (inner_inspector, _, _, _) = controlled_evm.inspector.into_parts();
        *self = Evm {
            ctx: controlled_evm.ctx,
            inspector: inner_inspector,
            instruction: controlled_evm.instruction,
            precompiles: controlled_evm.precompiles,
            frame_stack: controlled_evm.frame_stack,
        };

        Ok(ControlledExecutionResult::Completed(result))
    }
}

// Implementing InspectCommitEvm for Evm
impl<CTX, INSP, INST, PRECOMPILES> InspectCommitEvm
    for Evm<CTX, INSP, INST, PRECOMPILES, EthFrame<EthInterpreter>>
where
    CTX: ContextSetters
        + ContextTr<Journal: JournalTr<State = EvmState> + JournalExt, Db: DatabaseCommit>,
    INSP: Inspector<CTX, EthInterpreter>,
    INST: InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    PRECOMPILES: PrecompileProvider<CTX, Output = InterpreterResult>,
{
}

// Implementing InspectSystemCallEvm for Evm
impl<CTX, INSP, INST, PRECOMPILES> InspectSystemCallEvm
    for Evm<CTX, INSP, INST, PRECOMPILES, EthFrame<EthInterpreter>>
where
    CTX: ContextSetters
        + ContextTr<Journal: JournalTr<State = EvmState> + JournalExt, Tx: SystemCallTx>,
    INSP: Inspector<CTX, EthInterpreter>,
    INST: InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    PRECOMPILES: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn inspect_one_system_call_with_caller(
        &mut self,
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Result<Self::ExecutionResult, Self::Error> {
        // Set system call transaction fields similar to transact_system_call_with_caller
        self.set_tx(CTX::Tx::new_system_tx_with_caller(
            caller,
            system_contract_address,
            data,
        ));
        // Use inspect_run_system_call instead of run_system_call for inspection
        MainnetHandler::default().inspect_run_system_call(self)
    }
}

// Implementing InspectorEvmTr for Evm
impl<CTX, INSP, I, P> InspectorEvmTr for Evm<CTX, INSP, I, P, EthFrame<EthInterpreter>>
where
    CTX: ContextTr<Journal: JournalExt> + ContextSetters,
    I: InstructionProvider<Context = CTX, InterpreterTypes = EthInterpreter>,
    P: PrecompileProvider<CTX, Output = InterpreterResult>,
    INSP: Inspector<CTX, I::InterpreterTypes>,
{
    type Inspector = INSP;

    fn all_inspector(
        &self,
    ) -> (
        &Self::Context,
        &Self::Instructions,
        &Self::Precompiles,
        &FrameStack<Self::Frame>,
        &Self::Inspector,
    ) {
        let ctx = &self.ctx;
        let frame = &self.frame_stack;
        let instructions = &self.instruction;
        let precompiles = &self.precompiles;
        let inspector = &self.inspector;
        (ctx, instructions, precompiles, frame, inspector)
    }
    fn all_mut_inspector(
        &mut self,
    ) -> (
        &mut Self::Context,
        &mut Self::Instructions,
        &mut Self::Precompiles,
        &mut FrameStack<Self::Frame>,
        &mut Self::Inspector,
    ) {
        let ctx = &mut self.ctx;
        let frame = &mut self.frame_stack;
        let instructions = &mut self.instruction;
        let precompiles = &mut self.precompiles;
        let inspector = &mut self.inspector;
        (ctx, instructions, precompiles, frame, inspector)
    }
}
