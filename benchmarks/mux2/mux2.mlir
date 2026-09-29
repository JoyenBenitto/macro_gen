// 2:1 mux from AND/OR/NOT, y = (a & ~s) | (b & s), gate by gate.
hw.module @mux2(in %a: i1, in %b: i1, in %s: i1, out y: i1) {
  %true = hw.constant true
  %sn = comb.xor %s, %true : i1
  %0 = comb.and %a, %sn : i1
  %1 = comb.and %b, %s : i1
  %2 = comb.or %0, %1 : i1
  hw.output %2 : i1
}
