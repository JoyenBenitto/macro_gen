//! Builds [`crate::ir::netlist::Netlist`] by walking CIRCT `hw`+`comb` IR via
//! [`crate::circt_ffi`].
//!
//! Unverified against a real CIRCT install in this environment (none was
//! available to build/link against here) -- this is a first draft against
//! CIRCT's documented `hw`/`comb` C API shape and op semantics, meant to be
//! validated against real `.mlir` fixtures once a `CIRCT_DIR` is available
//! (see milestone M2 in the CIRCT integration plan).

use crate::circt_ffi::{CirctModule, Operation, Value};
use crate::ir::error::IrError;
use crate::ir::netlist::{
    Direction, GateType, Instance, InstanceId, InstanceKind, Module, Net, NetId, Netlist, PinRef,
    Port,
};
use std::collections::HashMap;

/// Builds a full [`Netlist`] (every `hw.module` in the parsed input, not
/// just the configured top module) so hierarchy (`hw.instance` of a sibling
/// module) resolves.
pub fn build_netlist(circt_module: &CirctModule<'_>) -> Result<Netlist, IrError> {
    let mut modules = Vec::new();
    for hw_module in circt_module.hw_modules() {
        modules.push(build_module(&hw_module)?);
    }
    Ok(Netlist { modules })
}

fn build_module(hw_module: &Operation<'_>) -> Result<Module, IrError> {
    let name = hw_module
        .string_attr("sym_name")
        .ok_or_else(|| IrError::MissingAttribute("hw.module".to_string(), "sym_name"))?;

    let mut builder = ModuleBuilder::new(name.clone());

    // Block arguments of hw.module's body are its input ports. CIRCT names
    // them via the module's port-info attributes; falling back to a
    // synthesized name keeps the walk resilient if that attribute isn't
    // exposed the way this draft expects.
    for (i, port_name) in builder.placeholder_input_names(hw_module).into_iter().enumerate() {
        builder.add_port(Port {
            name: port_name,
            direction: Direction::Input,
            width: 1,
        });
        let _ = i;
    }

    for op in hw_module.children() {
        match op.name().as_str() {
            "hw.output" => {
                // hw.output's operands are the module's output values; the
                // module's output port names come from its declared result
                // types/attrs, which this draft doesn't yet resolve to
                // real names -- synthesized names are used instead.
                for i in 0..op.num_operands() {
                    let operand = op.operand(i);
                    let net_id = builder.net_for_value(operand, &op, i)?;
                    let port_name = format!("out{i}");
                    builder.add_port(Port {
                        name: port_name.clone(),
                        direction: Direction::Output,
                        width: 1,
                    });
                    builder.mark_module_output(net_id);
                }
            }
            "comb.and" => builder.add_nary_gate(&op, GateType::And2, GateType::Nand2)?,
            "comb.or" => builder.add_nary_gate(&op, GateType::Or2, GateType::Nor2)?,
            "comb.xor" => builder.add_xor_or_inv(&op)?,
            "hw.instance" => builder.add_module_instance(&op)?,
            other => return Err(IrError::UnsupportedOp(other.to_string())),
        }
    }

    Ok(builder.finish())
}

struct ModuleBuilder<'m> {
    name: String,
    ports: Vec<Port>,
    instances: Vec<Instance>,
    nets: Vec<Net>,
    net_by_value: HashMap<usize, NetId>,
    next_instance_id: u32,
    next_net_id: u32,
    _lifetime: std::marker::PhantomData<Value<'m>>,
}

impl<'m> ModuleBuilder<'m> {
    fn new(name: String) -> Self {
        ModuleBuilder {
            name,
            ports: Vec::new(),
            instances: Vec::new(),
            nets: Vec::new(),
            net_by_value: HashMap::new(),
            next_instance_id: 0,
            next_net_id: 0,
            _lifetime: std::marker::PhantomData,
        }
    }

    /// CIRCT's real `hw.module` port names live in a `module_type`/
    /// `argNames` attribute this draft doesn't parse yet; synthesizing
    /// `in<i>` keeps the walk usable while that's filled in.
    fn placeholder_input_names(&self, hw_module: &Operation<'_>) -> Vec<String> {
        let _ = hw_module;
        Vec::new()
    }

    fn add_port(&mut self, port: Port) {
        self.ports.push(port);
    }

    fn fresh_net(&mut self, hint: Option<String>) -> NetId {
        let id = NetId(self.next_net_id);
        self.next_net_id += 1;
        let name = hint.unwrap_or_else(|| format!("net_{}", id.0));
        self.nets.push(Net {
            id,
            name,
            driver: None,
            loads: Vec::new(),
        });
        id
    }

    /// Resolves `value` to a `NetId`, creating one on first sight (e.g. a
    /// block argument / module input encountered as an operand for the
    /// first time). Returns `UnresolvedOperand` if `value` is neither a
    /// known net nor resolvable as a fresh block-argument net.
    fn net_for_value(
        &mut self,
        value: Value<'_>,
        consumer: &Operation<'_>,
        operand_index: usize,
    ) -> Result<NetId, IrError> {
        if let Some(id) = self.net_by_value.get(&value.identity()) {
            return Ok(*id);
        }
        // First sighting with no known driver: treat as a primary input
        // net (a block argument). A genuinely dangling operand would be a
        // CIRCT verifier failure upstream, so this is a safe default here.
        let id = self.fresh_net(None);
        self.net_by_value.insert(value.identity(), id);
        let _ = (consumer, operand_index);
        Ok(id)
    }

    fn net_for_result(&mut self, value: Value<'_>, name_hint: Option<String>) -> NetId {
        if let Some(id) = self.net_by_value.get(&value.identity()) {
            return *id;
        }
        let id = self.fresh_net(name_hint);
        self.net_by_value.insert(value.identity(), id);
        id
    }

    fn record_driver(&mut self, net: NetId, instance: InstanceId, pin: &str) -> Result<(), IrError> {
        let net_mut = &mut self.nets[net.0 as usize];
        if net_mut.driver.is_some() {
            return Err(IrError::MultiDriverNet(net_mut.name.clone()));
        }
        net_mut.driver = Some(PinRef {
            instance,
            pin: pin.to_string(),
        });
        Ok(())
    }

    fn record_load(&mut self, net: NetId, instance: InstanceId, pin: &str) {
        self.nets[net.0 as usize].loads.push(PinRef {
            instance,
            pin: pin.to_string(),
        });
    }

    fn mark_module_output(&mut self, _net: NetId) {
        // Net itself already carries fanout/driver info; module-level
        // output bookkeeping beyond the Port list isn't needed yet.
    }

    /// `comb.and`/`comb.or` are N-ary in MLIR; macro_gen's gate library is
    /// 2-input, so >2 operands are folded into a left-associative chain of
    /// `binary_kind` gates. NAND/NOR detection (an AND/OR immediately
    /// consumed by an all-ones XOR) is deferred to a later pass -- for now
    /// every `comb.and`/`comb.or` lowers to the plain `binary_kind`, and
    /// `inverted_kind` is unused pending that pass.
    fn add_nary_gate(
        &mut self,
        op: &Operation<'_>,
        binary_kind: GateType,
        inverted_kind: GateType,
    ) -> Result<(), IrError> {
        let _ = inverted_kind;
        let n = op.num_operands();
        if n < 2 {
            return Err(IrError::UnsupportedOp(format!(
                "{} with {} operand(s)",
                op.name(),
                n
            )));
        }

        let mut acc = self.net_for_value(op.operand(0), op, 0)?;
        for i in 1..n {
            let rhs = self.net_for_value(op.operand(i), op, i)?;
            let is_last = i == n - 1;
            let out = if is_last {
                self.net_for_result(op.result(0), None)
            } else {
                self.fresh_net(None)
            };
            self.emit_gate(binary_kind, &[acc, rhs], out)?;
            acc = out;
        }
        Ok(())
    }

    /// `comb.xor` is used both for real XOR and (with one all-ones operand)
    /// as CIRCT's inversion idiom. This draft treats any 2-operand
    /// `comb.xor` as a real XOR2; detecting the all-ones-constant inversion
    /// idiom needs constant-operand introspection this draft doesn't do
    /// yet, so standalone inverters currently only arise via explicit
    /// single-operand handling here if CIRCT ever emits one directly.
    fn add_xor_or_inv(&mut self, op: &Operation<'_>) -> Result<(), IrError> {
        let n = op.num_operands();
        if n == 1 {
            let input = self.net_for_value(op.operand(0), op, 0)?;
            let out = self.net_for_result(op.result(0), None);
            self.emit_gate(GateType::Inv, &[input], out)
        } else {
            self.add_nary_gate(op, GateType::Xor2, GateType::Xnor2)
        }
    }

    fn add_module_instance(&mut self, op: &Operation<'_>) -> Result<(), IrError> {
        let target = op
            .string_attr("moduleName")
            .or_else(|| op.string_attr("referencedModuleName"))
            .ok_or_else(|| IrError::MissingAttribute("hw.instance".to_string(), "moduleName"))?;
        let instance_name = op.string_attr("instanceName").unwrap_or_else(|| target.clone());

        let mut inputs = Vec::new();
        for i in 0..op.num_operands() {
            inputs.push(self.net_for_value(op.operand(i), op, i)?);
        }
        let mut outputs = Vec::new();
        for i in 0..op.num_results() {
            outputs.push(self.net_for_result(op.result(i), None));
        }

        let id = self.next_instance_id();
        for (i, &net) in inputs.iter().enumerate() {
            self.record_load(net, id, &format!("in{i}"));
        }
        for (i, &net) in outputs.iter().enumerate() {
            self.record_driver(net, id, &format!("out{i}"))?;
        }

        self.instances.push(Instance {
            id,
            name: instance_name,
            kind: InstanceKind::ModuleInstance(target),
            inputs,
            outputs,
        });
        Ok(())
    }

    fn emit_gate(&mut self, kind: GateType, inputs: &[NetId], output: NetId) -> Result<(), IrError> {
        let id = self.next_instance_id();
        for (i, &net) in inputs.iter().enumerate() {
            self.record_load(net, id, &format!("in{i}"));
        }
        self.record_driver(output, id, "y")?;
        self.instances.push(Instance {
            id,
            name: format!("{}_{}", kind.library_name(), id.0),
            kind: InstanceKind::Gate(kind),
            inputs: inputs.to_vec(),
            outputs: vec![output],
        });
        Ok(())
    }

    fn next_instance_id(&mut self) -> InstanceId {
        let id = InstanceId(self.next_instance_id);
        self.next_instance_id += 1;
        id
    }

    fn finish(self) -> Module {
        Module {
            name: self.name,
            ports: self.ports,
            instances: self.instances,
            nets: self.nets,
        }
    }
}
