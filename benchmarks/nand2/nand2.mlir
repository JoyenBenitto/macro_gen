// y = ~(a & b). CIRCT has no NAND: AND + NOT, folded back into one NAND2
// stage by gate-simplify.
hw.module @nand2(in %a: i1, in %b: i1, out y: i1) {
  %true = hw.constant true
  %0 = comb.and %a, %b : i1
  %1 = comb.xor %0, %true : i1
  hw.output %1 : i1
}
