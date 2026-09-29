use thiserror::Error;

#[derive(Debug, Error)]
pub enum IrError {
    #[error("op '{0}' has no CIRCT/MLIR mapping to macro_gen's gate library")]
    UnsupportedOp(String),
    #[error("operand {1} of op '{0}' does not resolve to a known net (unreachable driver)")]
    UnresolvedOperand(String, usize),
    #[error("net '{0}' has more than one driver, which macro_gen's netlist IR does not allow")]
    MultiDriverNet(String),
    #[error("module '{0}' was not found in the design")]
    ModuleNotFound(String),
    #[error("module '{0}' is defined more than once")]
    DuplicateModule(String),
    #[error("port '{1}' of module '{0}' is {2} bits wide; only 1-bit ports are supported")]
    UnsupportedWidth(String, String, i64),
    #[error("op '{0}' is missing required attribute '{1}'")]
    MissingAttribute(String, &'static str),
    #[error("pin '{0}' is already connected to a net")]
    PinAlreadyConnected(String),
    #[error("net '{0}' still has pins connected and cannot be removed")]
    NetInUse(String),
    #[error("cell '{0}' cannot change from {1} to {2}: different number of pins")]
    GateArityMismatch(String, &'static str, &'static str),
    #[error("module '{0}' failed verification: {1}")]
    Verify(String, String),
}
