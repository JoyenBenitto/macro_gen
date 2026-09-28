use thiserror::Error;

#[derive(Debug, Error)]
pub enum IrError {
    #[error("op '{0}' has no CIRCT/MLIR mapping to macro_gen's gate library")]
    UnsupportedOp(String),
    #[error("operand {1} of op '{0}' does not resolve to a known net (unreachable driver)")]
    UnresolvedOperand(String, usize),
    #[error("net '{0}' has more than one driver, which macro_gen's netlist IR does not allow")]
    MultiDriverNet(String),
    #[error("hw.module '{0}' was not found in the parsed CIRCT input")]
    ModuleNotFound(String),
    #[error("op '{0}' is missing required attribute '{1}'")]
    MissingAttribute(String, &'static str),
}
