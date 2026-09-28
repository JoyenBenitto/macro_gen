//! Logical effort (`g`) and parasitic delay (`p`) for each gate, derived
//! from the reference inverter's beta ratio and the gate's transistor
//! topology -- nothing here is user-configured.
//!
//! Logical effort is defined relative to the reference inverter: `g` is the
//! gate's input capacitance divided by that of an inverter with the same
//! drive strength. With `r = Wp/Wn` of the reference inverter (measured
//! from the PDK, see `inverter::id_based_w_l_n_p_ratio`), a unit inverter
//! has input cap `1 + r`, and:
//!
//! - an n-input NAND has n-high NMOS stacks (each NMOS is n wide) and
//!   parallel PMOS (r wide): `g = (n + r) / (1 + r)`
//! - an n-input NOR has parallel NMOS (1 wide) and n-high PMOS stacks
//!   (each PMOS is n*r wide): `g = (1 + n*r) / (1 + r)`
//!
//! At the textbook r = 2 these give 4/3 (NAND2) and 5/3 (NOR2); at any
//! other process ratio they differ, which is why they're computed here
//! rather than tabulated.
//!
//! Parasitic delay `p` is the output-node diffusion capacitance relative
//! to an inverter's (`1 + r`), counting only the drains that touch the
//! output.
//!
//! Gates without a derived topology model here (XOR, XNOR, MUX) return
//! `None` from [`GateLibrary::get`], and sizing reports them as
//! unsupported rather than guessing.

use crate::ir::GateType;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateParams {
    pub logical_effort: f64,
    pub parasitic_delay: f64,
    /// Height of the NMOS series stack (each NMOS is this many times a
    /// unit inverter's Wn, to keep pull-down strength).
    pub nmos_stack: u32,
    /// Height of the PMOS series stack.
    pub pmos_stack: u32,
}

#[derive(Debug, Clone)]
pub struct GateLibrary {
    /// `r = Wp/Wn` of the reference inverter this library was derived from.
    beta_ratio: f64,
    table: HashMap<GateType, GateParams>,
}

impl GateLibrary {
    /// Derives every supported gate's `g`/`p` from `beta_ratio = Wp/Wn`.
    /// The caller must ensure `beta_ratio` is finite and positive.
    pub fn from_beta_ratio(beta_ratio: f64) -> Self {
        let r = beta_ratio;
        let inv = GateParams { logical_effort: 1.0, parasitic_delay: 1.0, nmos_stack: 1, pmos_stack: 1 };
        let nand2 = nand(2, r);
        let nor2 = nor(2, r);
        // AND2/OR2 are NAND2/NOR2 followed by an inverter: the input sees
        // only the first stage, so g is that stage's; the delay adds the
        // inverter's parasitic.
        let and2 = GateParams { parasitic_delay: nand2.parasitic_delay + inv.parasitic_delay, ..nand2 };
        let or2 = GateParams { parasitic_delay: nor2.parasitic_delay + inv.parasitic_delay, ..nor2 };

        let mut table = HashMap::new();
        table.insert(GateType::Inv, inv);
        table.insert(GateType::Nand2, nand2);
        table.insert(GateType::Nor2, nor2);
        table.insert(GateType::And2, and2);
        table.insert(GateType::Or2, or2);
        GateLibrary { beta_ratio, table }
    }

    pub fn beta_ratio(&self) -> f64 {
        self.beta_ratio
    }

    pub fn get(&self, gate: GateType) -> Option<&GateParams> {
        self.table.get(&gate)
    }
}

/// n-input NAND: n-high NMOS stack, parallel PMOS.
fn nand(n: u32, r: f64) -> GateParams {
    let nf = f64::from(n);
    // Output node: n PMOS drains (r wide each) + the top NMOS (n wide).
    let out_cap = nf * r + nf;
    GateParams {
        logical_effort: (nf + r) / (1.0 + r),
        parasitic_delay: out_cap / (1.0 + r),
        nmos_stack: n,
        pmos_stack: 1,
    }
}

/// n-input NOR: parallel NMOS, n-high PMOS stack.
fn nor(n: u32, r: f64) -> GateParams {
    let nf = f64::from(n);
    // Output node: n NMOS drains (1 wide each) + the top PMOS (n*r wide).
    let out_cap = nf + nf * r;
    GateParams {
        logical_effort: (1.0 + nf * r) / (1.0 + r),
        parasitic_delay: out_cap / (1.0 + r),
        nmos_stack: 1,
        pmos_stack: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_textbook_values_at_beta_two() {
        let lib = GateLibrary::from_beta_ratio(2.0);
        let nand2 = lib.get(GateType::Nand2).unwrap();
        let nor2 = lib.get(GateType::Nor2).unwrap();
        assert!((nand2.logical_effort - 4.0 / 3.0).abs() < 1e-12);
        assert!((nor2.logical_effort - 5.0 / 3.0).abs() < 1e-12);
        assert!((nand2.parasitic_delay - 2.0).abs() < 1e-12);
        assert!((nor2.parasitic_delay - 2.0).abs() < 1e-12);
        assert_eq!(lib.get(GateType::Inv).unwrap().logical_effort, 1.0);
        // AND2/OR2 = NAND2/NOR2 + inverter.
        assert!((lib.get(GateType::And2).unwrap().parasitic_delay - 3.0).abs() < 1e-12);
        assert!((lib.get(GateType::Or2).unwrap().logical_effort - 5.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn logical_effort_follows_the_reference_inverter_ratio() {
        let lib = GateLibrary::from_beta_ratio(2.68);
        let nand2 = lib.get(GateType::Nand2).unwrap();
        let nor2 = lib.get(GateType::Nor2).unwrap();
        assert!((nand2.logical_effort - (2.0 + 2.68) / 3.68).abs() < 1e-12);
        assert!((nor2.logical_effort - (1.0 + 2.0 * 2.68) / 3.68).abs() < 1e-12);
        // Parasitic delay is ratio-independent under this model.
        assert!((nand2.parasitic_delay - 2.0).abs() < 1e-12);
    }

    #[test]
    fn unmodeled_gates_are_absent() {
        let lib = GateLibrary::from_beta_ratio(2.0);
        for gate in [GateType::Xor2, GateType::Xnor2, GateType::Mux2] {
            assert!(lib.get(gate).is_none());
        }
    }
}
