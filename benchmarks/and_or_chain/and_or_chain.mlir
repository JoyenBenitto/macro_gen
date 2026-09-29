// y = (a & b) | c as a complex cell: non-inverting, so AOI21 + INV.
hw.module @and_or_chain(in %a: i1, in %b: i1, in %c: i1, out y: i1) attributes {macro_gen.cell = "complex"} {
  %0 = comb.and %a, %b : i1
  %1 = comb.or %0, %c : i1
  hw.output %1 : i1
}
