//! Series-parallel transistor networks and the CMOS stages built from them.

/// A series-parallel switch network between the stage output and a rail.
/// `Fet(i)` is one transistor gated by stage input `i`. In a `Series`, the
/// first child sits next to the output and the last next to the rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    Fet(usize),
    Series(Vec<Network>),
    Parallel(Vec<Network>),
}

/// One end of a transistor: the stage output, the rail (GND for the
/// pull-down, VDD for the pull-up), or an internal stack node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Out,
    Rail,
    Internal(usize),
}

/// A transistor in a network, with its relative width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fet {
    pub input: usize,
    /// The end towards the output.
    pub upper: Node,
    /// The end towards the rail.
    pub lower: Node,
    pub width: f64,
}

impl Network {
    /// Swaps series and parallel: the pull-up network for a pull-down one.
    pub fn dual(&self) -> Network {
        match self {
            Network::Fet(i) => Network::Fet(*i),
            Network::Series(c) => Network::Parallel(c.iter().map(Network::dual).collect()),
            Network::Parallel(c) => Network::Series(c.iter().map(Network::dual).collect()),
        }
    }

    /// Every transistor, in depth-first order, sized so the worst path
    /// through the network conducts like one width-1 transistor: each of
    /// the `n` children of a `Series` gets `n` times the width, each child
    /// of a `Parallel` the full width. Internal nodes are numbered from 0.
    pub fn fets(&self) -> Vec<Fet> {
        let mut out = Vec::new();
        let mut next_node = 0;
        self.walk(1.0, Node::Out, Node::Rail, &mut next_node, &mut out);
        out
    }

    fn walk(&self, k: f64, upper: Node, lower: Node, next_node: &mut usize, out: &mut Vec<Fet>) {
        match self {
            Network::Fet(input) => out.push(Fet { input: *input, upper, lower, width: k }),
            Network::Parallel(children) => {
                for c in children {
                    c.walk(k, upper, lower, next_node, out);
                }
            }
            Network::Series(children) => {
                let n = children.len();
                let k = k * n as f64;
                let mut top = upper;
                for (i, c) in children.iter().enumerate() {
                    let bottom = if i + 1 == n {
                        lower
                    } else {
                        *next_node += 1;
                        Node::Internal(*next_node - 1)
                    };
                    c.walk(k, top, bottom, next_node, out);
                    top = bottom;
                }
            }
        }
    }
}

/// One static CMOS stage: inputs `in0..in{n-1}`, output `y = NOT(pdn)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmosStage {
    /// Library name, e.g. `nand2`; also the stem of its SPICE subckt.
    pub name: String,
    pub n_inputs: usize,
    pub pdn: Network,
}

impl CmosStage {
    pub fn new(name: impl Into<String>, n_inputs: usize, pdn: Network) -> Self {
        CmosStage { name: name.into(), n_inputs, pdn }
    }

    pub fn inv() -> Self {
        Self::new("inv", 1, Network::Fet(0))
    }

    pub fn nand(n: usize) -> Self {
        Self::new(format!("nand{n}"), n, Network::Series((0..n).map(Network::Fet).collect()))
    }

    pub fn nor(n: usize) -> Self {
        Self::new(format!("nor{n}"), n, Network::Parallel((0..n).map(Network::Fet).collect()))
    }

    pub fn pun(&self) -> Network {
        self.pdn.dual()
    }

    /// Pull-down transistors; width 1.0 = reference inverter NMOS.
    pub fn nmos(&self) -> Vec<Fet> {
        self.pdn.fets()
    }

    /// Pull-up transistors; width 1.0 = reference inverter PMOS.
    pub fn pmos(&self) -> Vec<Fet> {
        self.pun().fets()
    }

    /// Logical effort of input `i`: its gate capacitance relative to the
    /// reference inverter's (`1 + gamma`, in NMOS-width units).
    pub fn logical_effort(&self, input: usize, gamma: f64) -> f64 {
        let wn: f64 = self.nmos().iter().filter(|f| f.input == input).map(|f| f.width).sum();
        let wp: f64 = self.pmos().iter().filter(|f| f.input == input).map(|f| f.width).sum();
        (wn + gamma * wp) / (1.0 + gamma)
    }

    /// Parasitic delay: diffusion capacitance on the output node relative to
    /// the reference inverter's.
    pub fn parasitic(&self, gamma: f64) -> f64 {
        let at_out = |fets: Vec<Fet>| -> f64 {
            fets.iter().filter(|f| f.upper == Node::Out).map(|f| f.width).sum()
        };
        (at_out(self.nmos()) + gamma * at_out(self.pmos())) / (1.0 + gamma)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn inverter_is_the_unit() {
        let inv = CmosStage::inv();
        assert!(close(inv.logical_effort(0, 2.0), 1.0));
        assert!(close(inv.parasitic(2.0), 1.0));
    }

    #[test]
    fn nand_nor_match_textbook_logical_effort() {
        for gamma in [1.0, 2.0, 2.7] {
            let g_nand = CmosStage::nand(2).logical_effort(0, gamma);
            let g_nor = CmosStage::nor(2).logical_effort(1, gamma);
            assert!(close(g_nand, (2.0 + gamma) / (1.0 + gamma)));
            assert!(close(g_nor, (1.0 + 2.0 * gamma) / (1.0 + gamma)));
        }
        // gamma = 2: 4/3 and 5/3.
        assert!(close(CmosStage::nand(2).logical_effort(0, 2.0), 4.0 / 3.0));
        assert!(close(CmosStage::nor(2).logical_effort(0, 2.0), 5.0 / 3.0));
    }

    #[test]
    fn aoi21_widths_and_efforts() {
        // y = NOT((a AND b) OR c)
        let pdn = Network::Parallel(vec![
            Network::Series(vec![Network::Fet(0), Network::Fet(1)]),
            Network::Fet(2),
        ]);
        let aoi = CmosStage::new("aoi21", 3, pdn);
        let wn: Vec<(usize, f64)> = aoi.nmos().iter().map(|f| (f.input, f.width)).collect();
        assert_eq!(wn, vec![(0, 2.0), (1, 2.0), (2, 1.0)]);
        let wp: Vec<(usize, f64)> = aoi.pmos().iter().map(|f| (f.input, f.width)).collect();
        assert_eq!(wp, vec![(0, 2.0), (1, 2.0), (2, 2.0)]);
        // gamma = 2: g_a = (2 + 4)/3 = 2, g_c = (1 + 4)/3 = 5/3.
        assert!(close(aoi.logical_effort(0, 2.0), 2.0));
        assert!(close(aoi.logical_effort(2, 2.0), 5.0 / 3.0));
    }

    #[test]
    fn series_stacks_get_internal_nodes() {
        let fets = CmosStage::nand(2).nmos();
        assert_eq!(fets[0].upper, Node::Out);
        assert_eq!(fets[0].lower, Node::Internal(0));
        assert_eq!(fets[1].upper, Node::Internal(0));
        assert_eq!(fets[1].lower, Node::Rail);
        // Pull-up of a NAND is parallel: both straddle out <-> rail.
        assert!(CmosStage::nand(2).pmos().iter().all(|f| f.upper == Node::Out && f.lower == Node::Rail));
    }
}
