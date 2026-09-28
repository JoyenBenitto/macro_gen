//! Logical-effort path sizing math (Sutherland, Sproull & Harris,
//! *Logical Effort: Designing Fast CMOS Circuits*).
//!
//! Pure math, no FFI/CIRCT/config dependency -- deterministic and unit
//! tested against the textbook's worked examples, so these tests need no
//! ngspice or CIRCT toolchain and run in every `cargo test`.

use crate::sizing::gate_library::GateLibrary;

/// One stage of a combinational path: which gate it is, and the electrical
/// fanout it presents to the *next* stage (`C_load_of_next_stage /
/// C_in_of_next_stage`, i.e. how many "gate-equivalent" loads it drives).
/// For the path's *last* stage, `fanout_c` is instead the ratio of the
/// path's external output load to this stage's own unit input capacitance
/// -- see [`path_effort`].
#[derive(Debug, Clone, Copy)]
pub struct PathStage {
    pub gate: crate::ir::GateType,
    /// Branching effort at this stage: 1.0 if this stage's output only
    /// feeds the next stage on the path, or `(on_path_C + off_path_C) /
    /// on_path_C` when it also fans out elsewhere.
    pub branching_effort: f64,
}

/// Path logical effort `G`: product of each stage's gate logical effort.
pub fn path_logical_effort(stages: &[PathStage], lib: &GateLibrary) -> f64 {
    stages
        .iter()
        .map(|s| {
            lib.get(s.gate)
                .map(|e| e.logical_effort)
                .unwrap_or(1.0)
        })
        .product()
}

/// Path branching effort `B`: product of each stage's `branching_effort`.
pub fn path_branching_effort(stages: &[PathStage]) -> f64 {
    stages.iter().map(|s| s.branching_effort).product()
}

/// Path effort `F = G * B * H`, given the path's electrical effort
/// `h = C_load / C_in_first_stage`.
pub fn path_effort(stages: &[PathStage], lib: &GateLibrary, electrical_effort: f64) -> f64 {
    path_logical_effort(stages, lib) * path_branching_effort(stages) * electrical_effort
}

/// Best (delay-minimizing) per-stage effort `f_hat = F^(1/N)` for an
/// `N`-stage path with total path effort `path_effort`.
pub fn stage_effort_best_number(path_effort: f64, num_stages: usize) -> f64 {
    if num_stages == 0 {
        return 1.0;
    }
    path_effort.powf(1.0 / num_stages as f64)
}

/// Path delay in units of `tau`: `N * f_hat + sum(p_i)`.
pub fn path_delay(stages: &[PathStage], lib: &GateLibrary, f_hat: f64) -> f64 {
    let parasitic: f64 = stages
        .iter()
        .map(|s| {
            lib.get(s.gate)
                .map(|e| e.parasitic_delay)
                .unwrap_or(0.0)
        })
        .sum();
    stages.len() as f64 * f_hat + parasitic
}

/// Per-stage sizing result: the stage's total gate input capacitance, in
/// the same normalized units as `c_in_first_stage`/`c_out_load` (multiples
/// of a unit inverter's input cap). Converting this to physical W in
/// microns is [`crate::sizing::width_from_capacitance`]'s job, once an
/// electrical basis (`cox`, reference `Cin`) is available.
#[derive(Debug, Clone, Copy)]
pub struct GateCapacitance {
    pub gate: crate::ir::GateType,
    pub c_in: f64,
}

/// Back-solves each stage's input capacitance working from the path's
/// external output load backward to the first stage, given the path's
/// best stage effort `f_hat`. `c_out_load` is the load presented at the
/// *last* stage's output, normalized the same way as `stages`' branching
/// efforts (multiples of a unit gate's input cap).
///
/// Standard logical-effort back-solve: for stage `i` (0-indexed, last
/// stage = N-1), `C_in[i] = g[i] * C_out[i] / f_hat`, where `C_out[i]` is
/// the load stage `i` drives -- the external load for the last stage, or
/// `branching_effort[i] * C_in[i+1]` for every earlier stage.
pub fn back_solve_capacitances(
    stages: &[PathStage],
    lib: &GateLibrary,
    f_hat: f64,
    c_out_load: f64,
) -> Vec<GateCapacitance> {
    let mut results = vec![
        GateCapacitance {
            gate: crate::ir::GateType::Inv,
            c_in: 0.0
        };
        stages.len()
    ];

    let mut c_out_next = c_out_load;
    for i in (0..stages.len()).rev() {
        let g = lib
            .get(stages[i].gate)
            .map(|e| e.logical_effort)
            .unwrap_or(1.0);
        let c_in = g * c_out_next / f_hat.max(f64::EPSILON);
        results[i] = GateCapacitance {
            gate: stages[i].gate,
            c_in,
        };
        // The load the *previous* stage sees is this stage's C_in scaled
        // by this stage's branching effort (how much of its output fans
        // off the critical path).
        c_out_next = c_in * stages[i].branching_effort;
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::GateType;

    fn lib() -> GateLibrary {
        // r = 2 is the textbook beta ratio, so the expected values below
        // are the textbook g/p numbers.
        GateLibrary::from_beta_ratio(2.0)
    }

    /// Textbook FO4 example: a single inverter driving a load of 4x its
    /// own input capacitance (no branching). G=1 (inverter), B=1, H=4 ->
    /// F=4, N=1 -> f_hat = 4^(1/1) = 4.
    #[test]
    fn single_inverter_fo4() {
        let stages = [PathStage { gate: GateType::Inv, branching_effort: 1.0 }];
        let lib = lib();

        let g = path_logical_effort(&stages, &lib);
        let b = path_branching_effort(&stages);
        let f = path_effort(&stages, &lib, 4.0);
        let f_hat = stage_effort_best_number(f, stages.len());

        assert_eq!(g, 1.0);
        assert_eq!(b, 1.0);
        assert!((f - 4.0).abs() < 1e-9);
        assert!((f_hat - 4.0).abs() < 1e-9);
    }

    /// Textbook 2-stage NAND2 + INV path, electrical effort H=1 (driving a
    /// unit-sized load), no branching: G = g_nand2 * g_inv = 4/3 * 1 =
    /// 4/3. F = G*B*H = 4/3. f_hat = (4/3)^(1/2) ~= 1.1547.
    #[test]
    fn nand2_then_inverter_path_effort() {
        let stages = [
            PathStage { gate: GateType::Nand2, branching_effort: 1.0 },
            PathStage { gate: GateType::Inv, branching_effort: 1.0 },
        ];
        let lib = lib();

        let g = path_logical_effort(&stages, &lib);
        assert!((g - 4.0 / 3.0).abs() < 1e-9);

        let f = path_effort(&stages, &lib, 1.0);
        assert!((f - 4.0 / 3.0).abs() < 1e-9);

        let f_hat = stage_effort_best_number(f, 2);
        assert!((f_hat - (4.0_f64 / 3.0).sqrt()).abs() < 1e-9);
    }

    /// A 4-stage inverter chain (all g=1, no branching) driving H=16:
    /// F = 16, N=4 -> f_hat = 16^(1/4) = 2. Every stage's C_in should be
    /// exactly half the next stage's (constant taper).
    #[test]
    fn four_stage_inverter_chain_constant_taper() {
        let stages = [PathStage { gate: GateType::Inv, branching_effort: 1.0 }; 4];
        let lib = lib();

        let f = path_effort(&stages, &lib, 16.0);
        let f_hat = stage_effort_best_number(f, 4);
        assert!((f_hat - 2.0).abs() < 1e-9);

        let sized = back_solve_capacitances(&stages, &lib, f_hat, 16.0);
        for w in sized.windows(2) {
            assert!((w[1].c_in / w[0].c_in - 2.0).abs() < 1e-6);
        }
        // First stage's C_in should be 1.0 (its input is the path's
        // primary input, i.e. it's the "unit" stage this whole chain is
        // normalized against); sizes then double each stage toward the
        // 16x output load.
        assert!((sized[0].c_in - 1.0).abs() < 1e-6);
        assert!((sized[3].c_in - 8.0).abs() < 1e-6);
    }

    #[test]
    fn path_delay_sums_stage_effort_and_parasitics() {
        let stages = [
            PathStage { gate: GateType::Nand2, branching_effort: 1.0 },
            PathStage { gate: GateType::Inv, branching_effort: 1.0 },
        ];
        let lib = lib();
        let f_hat = 2.0;
        let d = path_delay(&stages, &lib, f_hat);
        // 2 stages * f_hat=2 + (p_nand2=2 + p_inv=1) = 4 + 3 = 7 tau.
        assert!((d - 7.0).abs() < 1e-9);
    }

    #[test]
    fn branching_effort_multiplies_into_path_effort() {
        let stages = [PathStage { gate: GateType::Inv, branching_effort: 2.0 }];
        let lib = lib();
        // G=1, B=2, H=3 -> F=6.
        let f = path_effort(&stages, &lib, 3.0);
        assert!((f - 6.0).abs() < 1e-9);
    }
}
