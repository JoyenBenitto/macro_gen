// OAI22 complex cell, y = ~((a | b) & (c | d)): one stage with 2-high
// stacks in both networks.
hw.module @oai22(in %a: i1, in %b: i1, in %c: i1, in %d: i1, out y: i1) attributes {macro_gen.cell = "complex"} {
  %true = hw.constant true
  %0 = comb.or %a, %b : i1
  %1 = comb.or %c, %d : i1
  %2 = comb.and %0, %1 : i1
  %3 = comb.xor %2, %true : i1
  hw.output %3 : i1
}
