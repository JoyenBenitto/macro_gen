//! Builds a [`Design`] by walking CIRCT `hw`+`comb` IR via
//! [`crate::circt_ffi`].
//!
//! Two passes over the input:
//! 1. every `hw.module` becomes an empty [`Module`](crate::ir::Module) with
//!    its ports (from `module_type`) and discardable attributes, so an
//!    `hw.instance` can resolve its target whatever the definition order;
//! 2. each body is walked op by op into gates, instances and nets.

use crate::circt_ffi::{CirctModule, Operation, Value};
use crate::ir::cell::GATE_OUTPUT_PIN;
use crate::ir::{Design, Direction, GateType, IrError, ModuleId, NetId, PinOwner};
use std::collections::{HashMap, HashSet};

/// Imports every `hw.module` in the input. The caller picks the top
/// ([`Design::set_top`]). The result has passed [`Design::verify`].
pub fn build_design(circt_module: &CirctModule<'_>) -> Result<Design, IrError> {
    let mut design = Design::new();
    let hw_modules: Vec<Operation<'_>> = circt_module.hw_modules().collect();

    let mut ids = Vec::with_capacity(hw_modules.len());
    for op in &hw_modules {
        ids.push(declare_module(&mut design, op)?);
    }
    for (op, &id) in hw_modules.iter().zip(&ids) {
        BodyBuilder::new(&mut design, id).build(op)?;
    }

    design.verify()?;
    Ok(design)
}

/// Pass 1: creates the module with its ports and attributes, no body.
fn declare_module(design: &mut Design, op: &Operation<'_>) -> Result<ModuleId, IrError> {
    let name = op
        .string_attr("sym_name")
        .ok_or_else(|| IrError::MissingAttribute("hw.module".to_string(), "sym_name"))?;
    let ports = op
        .module_ports()
        .ok_or_else(|| IrError::MissingAttribute(format!("hw.module @{name}"), "module_type"))?;

    let id = design.add_module(name.clone())?;
    let module = design.module_mut(id);
    module.attrs.extend(op.discardable_string_attrs());
    for (list, dir) in [(&ports.inputs, Direction::Input), (&ports.outputs, Direction::Output)] {
        for port in list {
            if port.width != 1 {
                return Err(IrError::UnsupportedWidth(name, port.name.clone(), port.width));
            }
            module.add_port(port.name.clone(), dir);
        }
    }
    Ok(id)
}

/// Pass 2 state for one module body.
struct BodyBuilder<'d> {
    design: &'d mut Design,
    module: ModuleId,
    /// MLIR SSA value (by [`Value::identity`]) → the net carrying it.
    net_by_value: HashMap<usize, NetId>,
    /// Nets already renamed after an output port (only the first port wins).
    named_by_output: HashSet<NetId>,
    next_cell: u32,
    next_net: u32,
}

impl<'d> BodyBuilder<'d> {
    fn new(design: &'d mut Design, module: ModuleId) -> Self {
        BodyBuilder {
            design,
            module,
            net_by_value: HashMap::new(),
            named_by_output: HashSet::new(),
            next_cell: 0,
            next_net: 0,
        }
    }

    fn build(mut self, hw_module: &Operation<'_>) -> Result<(), IrError> {
        // Input ports: the body's block arguments, in port order. Each gets
        // a net named after its port.
        let input_ports: Vec<_> = self.ports_of(Direction::Input);
        for (value, port) in hw_module.body_arguments().into_iter().zip(input_ports) {
            let m = self.design.module_mut(self.module);
            let port_name = m.ports()[port].name.clone();
            let net = m.add_net(port_name);
            m.connect(m.ports()[port].pin, net)?;
            self.net_by_value.insert(value.identity(), net);
        }

        for op in hw_module.children() {
            match op.name().as_str() {
                "comb.and" => self.add_nary_gate(&op, GateType::And2)?,
                "comb.or" => self.add_nary_gate(&op, GateType::Or2)?,
                "comb.xor" => self.add_nary_gate(&op, GateType::Xor2)?,
                "hw.instance" => self.add_instance(&op)?,
                "hw.output" => self.connect_outputs(&op)?,
                other => return Err(IrError::UnsupportedOp(other.to_string())),
            }
        }
        Ok(())
    }

    fn ports_of(&self, dir: Direction) -> Vec<crate::ir::PortId> {
        let m = self.design.module(self.module);
        m.ports().iter().filter(|(_, p)| p.dir == dir).map(|(id, _)| id).collect()
    }

    fn fresh_net(&mut self) -> NetId {
        let name = format!("net_{}", self.next_net);
        self.next_net += 1;
        self.design.module_mut(self.module).add_net(name)
    }

    /// The net for `value`, creating it on first sight. `hw.module` bodies
    /// are graph regions, so an operand may be defined by a later op; the
    /// net created here is then reused when that op's result is seen.
    fn net_for(&mut self, value: Value<'_>) -> NetId {
        if let Some(&net) = self.net_by_value.get(&value.identity()) {
            return net;
        }
        let net = self.fresh_net();
        self.net_by_value.insert(value.identity(), net);
        net
    }

    /// `comb.and`/`or`/`xor` are N-ary in MLIR; the gate library is
    /// 2-input, so N operands become a left-associative chain of N-1 gates.
    fn add_nary_gate(&mut self, op: &Operation<'_>, gate: GateType) -> Result<(), IrError> {
        let n = op.num_operands();
        if n < 2 {
            return Err(IrError::UnsupportedOp(format!("{} with {n} operand(s)", op.name())));
        }
        let mut acc = self.net_for(op.operand(0));
        for i in 1..n {
            let rhs = self.net_for(op.operand(i));
            let out = if i == n - 1 { self.net_for(op.result(0)) } else { self.fresh_net() };
            self.emit_gate(gate, &[acc, rhs], out)?;
            acc = out;
        }
        Ok(())
    }

    fn emit_gate(&mut self, gate: GateType, inputs: &[NetId], output: NetId) -> Result<(), IrError> {
        let name = format!("{}_{}", gate.library_name(), self.next_cell);
        self.next_cell += 1;
        let m = self.design.module_mut(self.module);
        let cell = m.add_gate(name, gate);
        for (i, &net) in inputs.iter().enumerate() {
            let pin = m.cell_pin(cell, &GateType::input_pin_name(i)).expect("add_gate creates in<i>");
            m.connect(pin, net)?;
        }
        let y = m.cell_pin(cell, GATE_OUTPUT_PIN).expect("add_gate creates y");
        m.connect(y, output)
    }

    /// `hw.instance`: operands feed the target's input ports and results
    /// come from its output ports, both in port order -- the same order
    /// [`Design::add_instance`] gives the instance's pins.
    fn add_instance(&mut self, op: &Operation<'_>) -> Result<(), IrError> {
        let target_name = op
            .symbol_ref_attr("moduleName")
            .ok_or_else(|| IrError::MissingAttribute("hw.instance".to_string(), "moduleName"))?;
        let target = self
            .design
            .module_by_name(&target_name)
            .ok_or_else(|| IrError::ModuleNotFound(target_name.clone()))?;
        let name = op.string_attr("instanceName").unwrap_or_else(|| target_name.clone());

        let mut nets = Vec::new();
        for i in 0..op.num_operands() {
            nets.push(self.net_for(op.operand(i)));
        }
        for i in 0..op.num_results() {
            nets.push(self.net_for(op.result(i)));
        }

        let cell = self.design.add_instance(self.module, name.clone(), target);
        let m = self.design.module_mut(self.module);
        let pins = m.cells()[cell].pins.clone();
        if pins.len() != nets.len() {
            return Err(IrError::Verify(
                m.name.clone(),
                format!(
                    "hw.instance '{name}' has {} operands+results but '{target_name}' has {} ports",
                    nets.len(),
                    pins.len()
                ),
            ));
        }
        for (pin, net) in pins.into_iter().zip(nets) {
            m.connect(pin, net)?;
        }
        Ok(())
    }

    /// `hw.output`: operand `i` is the value of output port `i`. A net
    /// driven by logic inside the module takes the port's name.
    fn connect_outputs(&mut self, op: &Operation<'_>) -> Result<(), IrError> {
        let output_ports = self.ports_of(Direction::Output);
        for (i, port) in output_ports.into_iter().enumerate().take(op.num_operands()) {
            let net = self.net_for(op.operand(i));
            let m = self.design.module_mut(self.module);
            m.connect(m.ports()[port].pin, net)?;
            let driven_by_cell = m
                .driver(net)
                .is_some_and(|p| matches!(m.pins()[p].owner, PinOwner::Cell(_)));
            if driven_by_cell && self.named_by_output.insert(net) {
                let port_name = m.ports()[port].name.clone();
                m.rename_net(net, port_name);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circt_ffi::CirctContext;
    use crate::ir::CellKind;

    fn import(src: &str) -> Design {
        let ctx = CirctContext::new();
        let module = ctx.parse_str(src, "test").unwrap();
        build_design(&module).unwrap()
    }

    #[test]
    fn and_or_chain_has_named_ports_linked_to_nets() {
        let d = import(include_str!("../../examples/circt/and_or_chain.mlir"));
        let id = d.module_by_name("and_or_chain").unwrap();
        let m = d.module(id);

        let ports: Vec<_> = m.ports().iter().map(|(_, p)| (p.name.as_str(), p.dir)).collect();
        assert_eq!(
            ports,
            vec![
                ("a", Direction::Input),
                ("b", Direction::Input),
                ("c", Direction::Input),
                ("y", Direction::Output),
            ]
        );
        let net_name = |port: &str| {
            let net = m.port_net(m.port_by_name(port).unwrap()).unwrap();
            m.nets()[net].name.clone()
        };
        assert_eq!(net_name("a"), "a");
        assert_eq!(net_name("y"), "y");

        let cells: Vec<_> = m.cells().iter().map(|(_, c)| (c.name.as_str(), c.kind)).collect();
        assert_eq!(
            cells,
            vec![
                ("and2_0", CellKind::Gate(GateType::And2)),
                ("or2_1", CellKind::Gate(GateType::Or2)),
            ]
        );
        // The OR's output drives port y.
        let or = m.cells().ids()[1];
        assert_eq!(m.output_nets(or), vec![m.port_net(m.port_by_name("y").unwrap()).unwrap()]);
    }

    #[test]
    fn module_tag_is_carried_into_design() {
        let d = import(
            r#"hw.module @tagged(in %a: i1, in %b: i1, out y: i1) attributes {macro_gen.cell = "complex"} {
                 %0 = comb.and %a, %b : i1
                 hw.output %0 : i1
               }
               hw.module @plain(in %a: i1, in %b: i1, out y: i1) {
                 %0 = comb.or %a, %b : i1
                 hw.output %0 : i1
               }"#,
        );
        let tagged = d.module(d.module_by_name("tagged").unwrap());
        assert_eq!(tagged.attrs.get("macro_gen.cell").map(String::as_str), Some("complex"));
        assert!(tagged.is_complex_cell());
        // Inherent attributes like sym_name must not leak in.
        assert!(!tagged.attrs.contains_key("sym_name"));

        let plain = d.module(d.module_by_name("plain").unwrap());
        assert!(plain.attrs.is_empty());
        assert!(!plain.is_complex_cell());
    }

    #[test]
    fn hw_instance_resolves_to_module_id() {
        // `top` is defined before `leaf` on purpose.
        let d = import(
            r#"hw.module @top(in %x: i1, in %w: i1, out z: i1) {
                 %0 = hw.instance "u0" @leaf(a: %x: i1, b: %w: i1) -> (y: i1)
                 hw.output %0 : i1
               }
               hw.module @leaf(in %a: i1, in %b: i1, out y: i1) {
                 %0 = comb.and %a, %b : i1
                 hw.output %0 : i1
               }"#,
        );
        let leaf = d.module_by_name("leaf").unwrap();
        let top = d.module(d.module_by_name("top").unwrap());
        let (cell_id, cell) = top.cells().iter().next().unwrap();
        assert_eq!(cell.name, "u0");
        assert_eq!(cell.kind, CellKind::Instance(leaf));
        let y = top.cell_pin(cell_id, "y").unwrap();
        assert_eq!(top.pins()[y].net, top.port_net(top.port_by_name("z").unwrap()));
    }

    #[test]
    fn wide_ports_are_rejected() {
        let ctx = CirctContext::new();
        let module = ctx
            .parse_str("hw.module @w(in %a: i4, out y: i4) { hw.output %a : i4 }", "test")
            .unwrap();
        assert!(matches!(build_design(&module), Err(IrError::UnsupportedWidth(_, _, 4))));
    }
}
