// ISCAS-85 c17: six NAND2s, five inputs, two outputs, with reconvergent
// fanout (N11 and N16 each feed two gates).
//
//   N10 = NAND(N1, N3)    N11 = NAND(N3, N6)
//   N16 = NAND(N2, N11)   N19 = NAND(N11, N7)
//   N22 = NAND(N10, N16)  N23 = NAND(N16, N19)
hw.module @c17(in %N1: i1, in %N2: i1, in %N3: i1, in %N6: i1, in %N7: i1, out N22: i1, out N23: i1) {
  %true = hw.constant true
  %a10 = comb.and %N1, %N3 : i1
  %N10 = comb.xor %a10, %true : i1
  %a11 = comb.and %N3, %N6 : i1
  %N11 = comb.xor %a11, %true : i1
  %a16 = comb.and %N2, %N11 : i1
  %N16 = comb.xor %a16, %true : i1
  %a19 = comb.and %N11, %N7 : i1
  %N19 = comb.xor %a19, %true : i1
  %a22 = comb.and %N10, %N16 : i1
  %N22 = comb.xor %a22, %true : i1
  %a23 = comb.and %N16, %N19 : i1
  %N23 = comb.xor %a23, %true : i1
  hw.output %N22, %N23 : i1, i1
}
