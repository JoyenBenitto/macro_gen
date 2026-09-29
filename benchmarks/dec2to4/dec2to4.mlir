// 2-to-4 decoder: four outputs, each input inverted and shared.
hw.module @dec2to4(in %a: i1, in %b: i1, out y0: i1, out y1: i1, out y2: i1, out y3: i1) {
  %true = hw.constant true
  %an = comb.xor %a, %true : i1
  %bn = comb.xor %b, %true : i1
  %0 = comb.and %an, %bn : i1
  %1 = comb.and %a, %bn : i1
  %2 = comb.and %an, %b : i1
  %3 = comb.and %a, %b : i1
  hw.output %0, %1, %2, %3 : i1, i1, i1, i1
}
