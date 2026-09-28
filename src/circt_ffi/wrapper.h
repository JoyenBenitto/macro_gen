// Umbrella header fed to bindgen (see build.rs::link_circt). Includes only
// the C API surface macro_gen's `hw`+`comb` structural walk needs: core
// MLIR context/module/operation/value plumbing, plus the two dialects.
//
// Intentionally excludes `mlir-c/Dialect/*` registration headers for
// dialects macro_gen doesn't read (e.g. `seq`) to keep generated bindings
// (and the resulting link requirements) small.

#include "mlir-c/IR.h"
#include "mlir-c/BuiltinAttributes.h"
#include "mlir-c/BuiltinTypes.h"
#include "mlir-c/Support.h"

#include "circt-c/Dialect/HW.h"
#include "circt-c/Dialect/Comb.h"
