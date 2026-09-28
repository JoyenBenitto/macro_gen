// A slightly larger combinational fixture with real fanout: %0 (the AND of
// a,b) drives two downstream gates, so the path enumerator sees a
// branching_effort > 1 net and the logical-effort sizer has to split load
// between two paths, not just chain gates 1:1.
//
//   a,b -> AND -> %0 -+-> OR(%0,c)  -> %1 -\
//                      |                    AND -> %3 (y)
//                      +-> XOR(%0,d) -> %2 -/
hw.module @comb_chain(in %a: i1, in %b: i1, in %c: i1, in %d: i1, out y: i1) {
  %0 = comb.and %a, %b : i1
  %1 = comb.or %0, %c : i1
  %2 = comb.xor %0, %d : i1
  %3 = comb.and %1, %2 : i1
  hw.output %3 : i1
}
