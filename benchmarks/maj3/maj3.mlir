// Full-adder carry / 3-input majority, co = ab | ac | bc, as a complex
// cell: one inverting majority stage + INV.
hw.module @maj3(in %a: i1, in %b: i1, in %c: i1, out co: i1) attributes {macro_gen.cell = "complex"} {
  %0 = comb.and %a, %b : i1
  %1 = comb.and %a, %c : i1
  %2 = comb.and %b, %c : i1
  %3 = comb.or %0, %1, %2 : i1
  hw.output %3 : i1
}
