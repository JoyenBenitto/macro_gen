// Single inverter, y = ~a: the classic buffering benchmark. On its own it
// drives the whole load in one stage; --add-buffer grows it into a
// log4(F)-stage chain.
hw.module @inv(in %a: i1, out y: i1) {
  %true = hw.constant true
  %0 = comb.xor %a, %true : i1
  hw.output %0 : i1
}
