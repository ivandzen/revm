use crate::{ExecutionControl, Inspector};
use interpreter::{interpreter_types::LoopControl, Interpreter, InterpreterTypes};

/// Inspector wrapper that enforces execution controls.
#[derive(Clone, Debug)]
pub struct ControlledInspector<INSP> {
    inner: INSP,
    control: ExecutionControl,
    steps_in_run: u64,
    total_steps: u64,
    suspend_step_limit: Option<u64>,
}

impl<INSP> ControlledInspector<INSP> {
    pub fn new(inner: INSP, control: ExecutionControl, total_steps: u64) -> Self {
        Self {
            inner,
            control,
            steps_in_run: 0,
            total_steps,
            suspend_step_limit: None,
        }
    }

    pub fn into_parts(self) -> (INSP, u64, u64, Option<u64>) {
        (
            self.inner,
            self.steps_in_run,
            self.total_steps,
            self.suspend_step_limit,
        )
    }
}

impl<CTX, INSP, INTR> Inspector<CTX, INTR> for ControlledInspector<INSP>
where
    INSP: Inspector<CTX, INTR>,
    INTR: InterpreterTypes,
{
    fn initialize_interp(&mut self, interp: &mut Interpreter<INTR>, context: &mut CTX) {
        self.inner.initialize_interp(interp, context);
    }

    fn step(&mut self, interp: &mut Interpreter<INTR>, context: &mut CTX) {
        self.inner.step(interp, context);
    }

    fn step_end(&mut self, interp: &mut Interpreter<INTR>, context: &mut CTX) {
        self.inner.step_end(interp, context);

        self.steps_in_run = self.steps_in_run.saturating_add(1);
        self.total_steps = self.total_steps.saturating_add(1);

        if self.suspend_step_limit.is_some() {
            return;
        }

        if let Some(limit) = self.control.max_steps {
            if self.steps_in_run >= limit {
                self.suspend_step_limit = Some(limit);
                interp
                    .bytecode
                    .set_action(interpreter::InterpreterAction::new_suspend());
                return;
            }
        }
    }

    fn log(&mut self, context: &mut CTX, log: primitives::Log) {
        self.inner.log(context, log);
    }

    fn call(
        &mut self,
        context: &mut CTX,
        inputs: &mut interpreter::CallInputs,
    ) -> Option<interpreter::CallOutcome> {
        self.inner.call(context, inputs)
    }

    fn call_end(
        &mut self,
        context: &mut CTX,
        inputs: &interpreter::CallInputs,
        outcome: &mut interpreter::CallOutcome,
    ) {
        self.inner.call_end(context, inputs, outcome);
    }

    fn create(
        &mut self,
        context: &mut CTX,
        inputs: &mut interpreter::CreateInputs,
    ) -> Option<interpreter::CreateOutcome> {
        self.inner.create(context, inputs)
    }

    fn create_end(
        &mut self,
        context: &mut CTX,
        inputs: &interpreter::CreateInputs,
        outcome: &mut interpreter::CreateOutcome,
    ) {
        self.inner.create_end(context, inputs, outcome);
    }

    fn selfdestruct(
        &mut self,
        contract: primitives::Address,
        target: primitives::Address,
        value: primitives::U256,
    ) {
        self.inner.selfdestruct(contract, target, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoOpInspector;
    use interpreter::{interpreter::EthInterpreter, InterpreterAction};

    #[test]
    fn step_limit_sets_suspend_action_and_tracks_steps() {
        let mut inspector = ControlledInspector::new(
            NoOpInspector,
            ExecutionControl { max_steps: Some(1) },
            0,
        );
        let mut interp = Interpreter::<EthInterpreter>::default();
        let mut ctx = ();

        inspector.step_end(&mut interp, &mut ctx);

        assert!(matches!(
            interp.bytecode.action().as_ref(),
            Some(InterpreterAction::Suspend)
        ));

        let (_inner, steps_in_run, total_steps, step_limit) = inspector.into_parts();
        assert_eq!(steps_in_run, 1);
        assert_eq!(total_steps, 1);
        assert_eq!(step_limit, Some(1));
    }

    #[test]
    fn step_limit_is_recorded_once() {
        let mut inspector = ControlledInspector::new(
            NoOpInspector,
            ExecutionControl { max_steps: Some(1) },
            0,
        );
        let mut interp = Interpreter::<EthInterpreter>::default();
        let mut ctx = ();

        inspector.step_end(&mut interp, &mut ctx);
        inspector.step_end(&mut interp, &mut ctx);

        let (_inner, steps_in_run, total_steps, step_limit) = inspector.into_parts();
        assert_eq!(steps_in_run, 2);
        assert_eq!(total_steps, 2);
        assert_eq!(step_limit, Some(1));
    }
}
