//! Builds per-path [`PathStage`](crate::sizing::logical_effort::PathStage)
//! sequences by walking a [`Netlist`]'s fanout graph.
//!
//! v1 simplification (documented, not hidden): each root-to-leaf
//! combinational path is sized independently, rather than doing full
//! critical-path search across reconverging paths. This matches per-path
//! logical-effort sizing as commonly taught, and is a reasonable starting
//! point for a netlist that's mostly a tree; a later milestone can add
//! critical-path selection for heavily reconverging designs.

use crate::ir::{GateType, Instance, InstanceKind, Module, NetId};
use crate::sizing::logical_effort::PathStage;

/// One full combinational path through `module`, from a primary input net
/// to a primary output net, as a sequence of gate stages.
#[derive(Debug, Clone)]
pub struct Path {
    pub stages: Vec<PathStage>,
    /// The `Instance` each stage in `stages` came from, same order/length.
    pub instances: Vec<crate::ir::InstanceId>,
}

/// Enumerates every root-to-leaf combinational path in `module` that
/// consists entirely of primitive gates (an `InstanceKind::ModuleInstance`
/// terminates path enumeration at that point -- hierarchy is sized
/// module-by-module, not flattened).
pub fn enumerate_paths(module: &Module) -> Vec<Path> {
    let mut paths = Vec::new();
    let primary_inputs: Vec<NetId> = module
        .nets
        .iter()
        .filter(|n| n.driver.is_none())
        .map(|n| n.id)
        .collect();

    for &start in &primary_inputs {
        walk(module, start, Vec::new(), Vec::new(), &mut paths);
    }
    paths
}

fn walk(
    module: &Module,
    net: NetId,
    stages: Vec<PathStage>,
    instances: Vec<crate::ir::InstanceId>,
    out: &mut Vec<Path>,
) {
    let loads = &module.nets[net.0 as usize].loads;
    if loads.is_empty() {
        // Dead end (unused net) -- nothing to record; a genuine primary
        // output is only reached through a gate whose output net has no
        // further loads, handled below via each gate's output net.
        return;
    }

    for load in loads {
        let Some(inst) = module.instances.iter().find(|i| i.id == load.instance) else {
            continue;
        };
        match &inst.kind {
            InstanceKind::Gate(gate) => {
                let branching = load_branching_effort(module, net);
                let mut next_stages = stages.clone();
                next_stages.push(PathStage {
                    gate: *gate,
                    branching_effort: branching,
                });
                let mut next_instances = instances.clone();
                next_instances.push(inst.id);

                // A gate's output net may itself fan out further (continue
                // the path) or have no loads (a primary output -- record
                // the completed path here).
                for &out_net in &inst.outputs {
                    if module.nets[out_net.0 as usize].loads.is_empty() {
                        out.push(Path {
                            stages: next_stages.clone(),
                            instances: next_instances.clone(),
                        });
                    } else {
                        walk(module, out_net, next_stages.clone(), next_instances.clone(), out);
                    }
                }
            }
            InstanceKind::ModuleInstance(_) => {
                // Hierarchy boundary: path enumeration stops here for this
                // v1 (each hw.module is sized independently).
            }
        }
    }
}

/// Branching effort of `net`: `(on_path + off_path load count) /
/// on_path load count`, approximating each load's input capacitance as
/// equal (reasonable until per-load sizes are known -- capacitance-aware
/// branching would need a second pass after an initial sizing estimate).
fn load_branching_effort(module: &Module, net: NetId) -> f64 {
    let n = module.nets[net.0 as usize].loads.len();
    if n <= 1 {
        1.0
    } else {
        n as f64
    }
}

/// Convenience: the primitive [`GateType`]s of every instance reachable in
/// `module` that isn't hierarchy -- used by [`crate::sizing`] to know which
/// gates were actually covered by path enumeration vs. left unsized (e.g.
/// an instance with no path reaching it, which shouldn't normally happen
/// for a fully-connected design but is worth being able to detect).
pub fn gate_instances(module: &Module) -> impl Iterator<Item = &Instance> {
    module.instances.iter().filter(|i| matches!(i.kind, InstanceKind::Gate(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Direction, InstanceId, Net, PinRef, Port};

    /// in -> INV -> out, a single-gate path.
    fn single_inverter_module() -> Module {
        Module {
            name: "inv1".to_string(),
            ports: vec![
                Port { name: "a".to_string(), direction: Direction::Input, width: 1 },
                Port { name: "y".to_string(), direction: Direction::Output, width: 1 },
            ],
            instances: vec![Instance {
                id: InstanceId(0),
                name: "u0".to_string(),
                kind: InstanceKind::Gate(GateType::Inv),
                inputs: vec![NetId(0)],
                outputs: vec![NetId(1)],
            }],
            nets: vec![
                Net {
                    id: NetId(0),
                    name: "a".to_string(),
                    driver: None,
                    loads: vec![PinRef { instance: InstanceId(0), pin: "in0".to_string() }],
                },
                Net {
                    id: NetId(1),
                    name: "y".to_string(),
                    driver: Some(PinRef { instance: InstanceId(0), pin: "y".to_string() }),
                    loads: vec![],
                },
            ],
        }
    }

    #[test]
    fn enumerates_single_inverter_path() {
        let module = single_inverter_module();
        let paths = enumerate_paths(&module);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].stages.len(), 1);
        assert!(matches!(paths[0].stages[0].gate, GateType::Inv));
        assert_eq!(paths[0].instances, vec![InstanceId(0)]);
    }

    #[test]
    fn gate_instances_skips_hierarchy() {
        let mut module = single_inverter_module();
        module.instances.push(Instance {
            id: InstanceId(1),
            name: "sub".to_string(),
            kind: InstanceKind::ModuleInstance("child".to_string()),
            inputs: vec![],
            outputs: vec![],
        });
        let gates: Vec<_> = gate_instances(&module).collect();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].id, InstanceId(0));
    }
}
