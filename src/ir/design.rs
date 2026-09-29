//! The whole design: every module definition plus which one is the top.
//! This is the single object the pass pipeline mutates and hands on.

use crate::ir::arena::{Arena, CellId, ModuleId};
use crate::ir::cell::CellKind;
use crate::ir::error::IrError;
use crate::ir::module::{Direction, Module};

#[derive(Debug, Clone, Default)]
pub struct Design {
    modules: Arena<ModuleId, Module>,
    top: Option<ModuleId>,
}

impl Design {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an empty module. Names must be unique across the design.
    pub fn add_module(&mut self, name: impl Into<String>) -> Result<ModuleId, IrError> {
        let name = name.into();
        if self.module_by_name(&name).is_some() {
            return Err(IrError::DuplicateModule(name));
        }
        Ok(self.modules.alloc(Module::new(name)))
    }

    pub fn modules(&self) -> &Arena<ModuleId, Module> {
        &self.modules
    }

    pub fn module(&self, id: ModuleId) -> &Module {
        &self.modules[id]
    }

    pub fn module_mut(&mut self, id: ModuleId) -> &mut Module {
        &mut self.modules[id]
    }

    pub fn module_by_name(&self, name: &str) -> Option<ModuleId> {
        self.modules.iter().find(|(_, m)| m.name == name).map(|(id, _)| id)
    }

    pub fn top(&self) -> Option<ModuleId> {
        self.top
    }

    pub fn set_top(&mut self, id: ModuleId) {
        self.top = Some(id);
    }

    /// Adds to `parent` an instance of `target`, with one unconnected pin
    /// per target port (same names and order).
    pub fn add_instance(
        &mut self,
        parent: ModuleId,
        name: impl Into<String>,
        target: ModuleId,
    ) -> CellId {
        let spec = port_spec(&self.modules[target]);
        self.modules[parent].add_instance(name, target, &spec)
    }

    /// Verifies every module, then cross-module facts: the top is live and
    /// every instance targets a live module whose ports match its pins.
    pub fn verify(&self) -> Result<(), IrError> {
        for (_, module) in self.modules.iter() {
            module.verify()?;
        }
        if let Some(top) = self.top {
            if !self.modules.contains(top) {
                return Err(IrError::Verify("<design>".into(), format!("top {top:?} was removed")));
            }
        }
        for (_, module) in self.modules.iter() {
            for (_, cell) in module.cells().iter() {
                let CellKind::Instance(target) = cell.kind else { continue };
                let Some(target_module) = self.modules.get(target) else {
                    return Err(IrError::Verify(
                        module.name.clone(),
                        format!("instance '{}' targets removed module {target:?}", cell.name),
                    ));
                };
                let pins: Vec<(String, Direction)> = cell
                    .pins
                    .iter()
                    .map(|&p| (module.pins()[p].name.clone(), module.pins()[p].dir))
                    .collect();
                if pins != port_spec(target_module) {
                    return Err(IrError::Verify(
                        module.name.clone(),
                        format!(
                            "instance '{}' pins do not match the ports of '{}'",
                            cell.name, target_module.name
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A module's ports as `(name, direction)`, in order.
fn port_spec(module: &Module) -> Vec<(String, Direction)> {
    module.ports().iter().map(|(_, p)| (p.name.clone(), p.dir)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_pins_follow_target_ports() {
        let mut d = Design::new();
        let leaf = d.add_module("leaf").unwrap();
        d.module_mut(leaf).add_port("a", Direction::Input);
        d.module_mut(leaf).add_port("y", Direction::Output);
        let top = d.add_module("top").unwrap();
        d.set_top(top);
        let inst = d.add_instance(top, "u0", leaf);

        let m = d.module(top);
        assert_eq!(m.cells()[inst].kind, CellKind::Instance(leaf));
        assert!(m.cell_pin(inst, "a").is_some() && m.cell_pin(inst, "y").is_some());
        d.verify().unwrap();

        // Changing the target's ports afterwards is caught.
        d.module_mut(leaf).add_port("b", Direction::Input);
        assert!(matches!(d.verify(), Err(IrError::Verify(..))));
    }

    #[test]
    fn duplicate_module_names_are_rejected() {
        let mut d = Design::new();
        d.add_module("m").unwrap();
        assert!(matches!(d.add_module("m"), Err(IrError::DuplicateModule(_))));
    }
}
