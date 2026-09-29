// 8-input AND as one N-ary comb.and. The importer decomposes it into a
// linear chain of AND2s, so this measures a deep, unbalanced path.
hw.module @and8(in %a: i1, in %b: i1, in %c: i1, in %d: i1, in %e: i1, in %f: i1, in %g: i1, in %h: i1, out y: i1) {
  %0 = comb.and %a, %b, %c, %d, %e, %f, %g, %h : i1
  hw.output %0 : i1
}
