//! Pass infrastructure: a [`Pass`] transforms the [`Design`] in place, and a
//! [`PassManager`] runs a pipeline of them in order, verifying the IR after
//! each one so a broken invariant is blamed on the pass that broke it.

pub mod dead_logic;

use crate::ir::{Design, IrError};
use log::info;
use thiserror::Error;

/// One transformation over the whole design: update, rewire, add or remove
/// nodes. Must leave the design valid ([`Design::verify`]).
pub trait Pass {
    fn name(&self) -> &'static str;
    fn run(&mut self, design: &mut Design) -> Result<(), IrError>;
}

#[derive(Debug, Error)]
pub enum PassError {
    #[error("pass '{pass}' failed: {source}")]
    Failed { pass: &'static str, source: IrError },
    #[error("IR is invalid after pass '{pass}': {source}")]
    InvalidAfter { pass: &'static str, source: IrError },
}

pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
    verify_each: bool,
}

impl Default for PassManager {
    fn default() -> Self {
        PassManager { passes: Vec::new(), verify_each: true }
    }
}

impl PassManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, pass: impl Pass + 'static) -> &mut Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// Whether to run [`Design::verify`] after every pass (default: on).
    pub fn verify_each(&mut self, on: bool) -> &mut Self {
        self.verify_each = on;
        self
    }

    pub fn pass_names(&self) -> Vec<&'static str> {
        self.passes.iter().map(|p| p.name()).collect()
    }

    pub fn run(&mut self, design: &mut Design) -> Result<(), PassError> {
        for pass in &mut self.passes {
            let name = pass.name();
            info!("Running pass '{name}'");
            pass.run(design).map_err(|source| PassError::Failed { pass: name, source })?;
            if self.verify_each {
                design.verify().map_err(|source| PassError::InvalidAfter { pass: name, source })?;
            }
        }
        Ok(())
    }
}

/// The pipeline run on every imported design. New passes are appended here.
pub fn default_pipeline() -> PassManager {
    let mut pm = PassManager::new();
    pm.add(dead_logic::DeadLogicElimination);
    pm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Direction, GateType};

    struct Record(&'static str, std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>);
    impl Pass for Record {
        fn name(&self) -> &'static str {
            self.0
        }
        fn run(&mut self, _: &mut Design) -> Result<(), IrError> {
            self.1.borrow_mut().push(self.0);
            Ok(())
        }
    }

    /// Leaves the design invalid in a way only `Design::verify` catches: an
    /// instance whose target module gains a port afterwards.
    struct Corrupt;
    impl Pass for Corrupt {
        fn name(&self) -> &'static str {
            "corrupt"
        }
        fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
            let leaf = design.add_module("leaf")?;
            let top = design.add_module("top")?;
            design.add_instance(top, "u0", leaf);
            design.module_mut(leaf).add_port("a", Direction::Input);
            Ok(())
        }
    }

    struct Fails;
    impl Pass for Fails {
        fn name(&self) -> &'static str {
            "fails"
        }
        fn run(&mut self, design: &mut Design) -> Result<(), IrError> {
            let m = design.add_module("m")?;
            let g = design.module_mut(m).add_gate("g", GateType::And2);
            design.module_mut(m).set_gate_type(g, GateType::Inv)
        }
    }

    #[test]
    fn runs_passes_in_order() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut pm = PassManager::new();
        pm.add(Record("first", log.clone())).add(Record("second", log.clone()));
        pm.run(&mut Design::new()).unwrap();
        assert_eq!(*log.borrow(), vec!["first", "second"]);
    }

    #[test]
    fn invalid_ir_is_blamed_on_the_pass() {
        let mut pm = PassManager::new();
        pm.add(Corrupt);
        match pm.run(&mut Design::new()) {
            Err(PassError::InvalidAfter { pass, .. }) => assert_eq!(pass, "corrupt"),
            other => panic!("expected InvalidAfter, got {other:?}"),
        }
    }

    #[test]
    fn pass_errors_carry_the_pass_name() {
        let mut pm = PassManager::new();
        pm.add(Fails);
        assert!(matches!(pm.run(&mut Design::new()), Err(PassError::Failed { pass: "fails", .. })));
    }
}
