// Minimal fixture for the `hw`+`comb` walker (src/ir/from_circt.rs):
// y = (a AND b) OR c
//
// Two primitive gates, no hierarchy, no constants -- exercises exactly the
// op set ir::from_circt currently handles (comb.and, comb.or, hw.output).
//
// `macro_gen.cell = "complex"` tags the whole module as a custom cell, to be
// built as a single pull-up/pull-down network instead of gate by gate.
hw.module @and_or_chain(in %a: i1, in %b: i1, in %c: i1, out y: i1) attributes {macro_gen.cell = "complex"} {
  %0 = comb.and %a, %b : i1
  %1 = comb.or %0, %c : i1
  hw.output %1 : i1
}
