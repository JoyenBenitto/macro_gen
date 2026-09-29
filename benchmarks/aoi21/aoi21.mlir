// AOI21 complex cell, y = ~((a & b) | c): one pull-up/pull-down stage.
hw.module @aoi21(in %a: i1, in %b: i1, in %c: i1, out y: i1) attributes {macro_gen.cell = "complex"} {
  %true = hw.constant true
  %0 = comb.and %a, %b : i1
  %1 = comb.or %0, %c : i1
  %2 = comb.xor %1, %true : i1
  hw.output %2 : i1
}
