//! Safe wrapper around CIRCT/MLIR's C API.
//!
//! Nothing outside this module should touch the raw bindgen types directly
//! (`allow(...)` below is scoped to the generated file for that reason).
//! `src/ir/from_circt.rs` is the only intended consumer.
//!
//! This module has not been exercised against a real CIRCT install in this
//! environment (none was available); it's written against CIRCT's documented
//! C API shape (`circt-c/Dialect/HW.h`, `circt-c/Dialect/Comb.h`, and MLIR's
//! own `mlir-c/IR.h`) and should be treated as a first draft to validate
//! against a real `CIRCT_DIR` before relying on it.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]

mod raw {
    include!(concat!(env!("OUT_DIR"), "/circt_bindings.rs"));
}

use std::ffi::{CStr, CString};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CirctError {
    #[error("failed to read '{0}': {1}")]
    Io(String, std::io::Error),
    #[error("'{0}' contains a NUL byte and cannot be passed to CIRCT's C API")]
    NulInPath(String),
    #[error("CIRCT/MLIR rejected '{0}' as invalid IR (parse failure)")]
    ParseFailed(String),
}

/// Owns an `MlirContext` for the lifetime of a parse; all `MlirModule`,
/// `MlirOperation`, and `MlirValue` handles borrowed from it are only valid
/// while this is alive.
pub struct CirctContext {
    ctx: raw::MlirContext,
}

impl CirctContext {
    pub fn new() -> Self {
        let ctx = unsafe { raw::mlirContextCreate() };
        // hw/comb (and the builtin dialect they depend on) must be
        // registered before parsing IR that uses their ops, or the parser
        // will reject every `hw.*`/`comb.*` op as unknown.
        unsafe {
            raw::mlirDialectHandleRegisterDialect(raw::mlirGetDialectHandle__hw__(), ctx);
            raw::mlirDialectHandleRegisterDialect(raw::mlirGetDialectHandle__comb__(), ctx);
            raw::mlirContextLoadAllAvailableDialects(ctx);
        }
        CirctContext { ctx }
    }

    /// Parses a textual `.mlir` file (`hw`+`comb` dialects) into a module.
    pub fn parse_file(&self, path: &std::path::Path) -> Result<CirctModule<'_>, CirctError> {
        let path_str = path.display().to_string();
        let source = std::fs::read_to_string(path)
            .map_err(|e| CirctError::Io(path_str.clone(), e))?;
        self.parse_str(&source, &path_str)
    }

    /// Parses in-memory textual MLIR, primarily for tests/fixtures.
    /// `source_name` is used only in error messages.
    pub fn parse_str<'ctx>(
        &'ctx self,
        source: &str,
        source_name: &str,
    ) -> Result<CirctModule<'ctx>, CirctError> {
        let c_source = CString::new(source)
            .map_err(|_| CirctError::NulInPath(source_name.to_string()))?;
        let module = unsafe {
            raw::mlirModuleCreateParse(self.ctx, mlir_string_ref(&c_source))
        };
        if module.ptr.is_null() {
            return Err(CirctError::ParseFailed(source_name.to_string()));
        }
        Ok(CirctModule {
            module,
            _ctx: std::marker::PhantomData,
        })
    }
}

impl Drop for CirctContext {
    fn drop(&mut self) {
        unsafe { raw::mlirContextDestroy(self.ctx) };
    }
}

pub struct CirctModule<'ctx> {
    module: raw::MlirModule,
    _ctx: std::marker::PhantomData<&'ctx CirctContext>,
}

impl<'ctx> CirctModule<'ctx> {
    /// Iterates every top-level `hw.module` operation in the parsed module.
    pub fn hw_modules(&self) -> impl Iterator<Item = Operation<'_>> + '_ {
        let root = unsafe { raw::mlirModuleGetOperation(self.module) };
        OperationChildren::of(root).filter(|op| op.name() == "hw.module")
    }
}

/// A single MLIR operation (e.g. one `hw.module`, one `comb.and`, one
/// `hw.instance`), borrowed from a [`CirctModule`].
#[derive(Clone, Copy)]
pub struct Operation<'m> {
    op: raw::MlirOperation,
    _module: std::marker::PhantomData<&'m ()>,
}

impl<'m> Operation<'m> {
    /// The op's mnemonic, e.g. `"hw.module"`, `"comb.and"`, `"hw.instance"`.
    pub fn name(&self) -> String {
        let ident = unsafe { raw::mlirOperationGetName(self.op) };
        mlir_string_ref_to_string(unsafe { raw::mlirIdentifierStr(ident) })
    }

    /// The `sym_name`/`instanceName` string attribute, when present --
    /// covers both `hw.module`'s `sym_name` and `hw.instance`'s
    /// `instanceName`.
    pub fn string_attr(&self, name: &str) -> Option<String> {
        let c_name = CString::new(name).ok()?;
        let attr = unsafe {
            raw::mlirOperationGetAttributeByName(self.op, mlir_string_ref(&c_name))
        };
        if attr.ptr.is_null() || !unsafe { raw::mlirAttributeIsAString(attr) } {
            return None;
        }
        let s = unsafe { raw::mlirStringAttrGetValue(attr) };
        Some(mlir_string_ref_to_string(s))
    }

    pub fn num_operands(&self) -> usize {
        unsafe { raw::mlirOperationGetNumOperands(self.op) as usize }
    }

    pub fn operand(&self, i: usize) -> Value<'m> {
        Value {
            value: unsafe { raw::mlirOperationGetOperand(self.op, i as isize) },
            _module: std::marker::PhantomData,
        }
    }

    pub fn num_results(&self) -> usize {
        unsafe { raw::mlirOperationGetNumResults(self.op) as usize }
    }

    pub fn result(&self, i: usize) -> Value<'m> {
        Value {
            value: unsafe { raw::mlirOperationGetResult(self.op, i as isize) },
            _module: std::marker::PhantomData,
        }
    }

    /// Direct child operations in this op's (single, for `hw.module`) body
    /// region -- e.g. every `comb.*`/`hw.instance`/`hw.output` inside an
    /// `hw.module`.
    pub fn children(&self) -> impl Iterator<Item = Operation<'m>> {
        OperationChildren::of(self.op)
    }
}

/// An SSA value: either an operation's result or a block argument (a
/// module port). Two `Value`s refer to the same net iff [`Value::identity`]
/// is equal -- MLIR values don't implement `Hash`/`Eq` themselves, so
/// `ir::from_circt` keys nets on this raw pointer instead.
#[derive(Clone, Copy)]
pub struct Value<'m> {
    value: raw::MlirValue,
    _module: std::marker::PhantomData<&'m ()>,
}

impl<'m> Value<'m> {
    /// Opaque identity suitable as a `HashMap` key for net deduplication.
    /// Not meaningful for anything else (not an index, not stable across
    /// parses).
    pub fn identity(&self) -> usize {
        self.value.ptr as usize
    }
}

/// Iterates the operations directly inside `op`'s first region's first (and
/// for the ops macro_gen reads -- `hw.module`, single-block regions --
/// only) block, following `mlirOperationGetNextInBlock`.
struct OperationChildren<'m> {
    current: raw::MlirOperation,
    started: bool,
    _module: std::marker::PhantomData<&'m ()>,
}

impl<'m> OperationChildren<'m> {
    fn of(op: raw::MlirOperation) -> Self {
        let num_regions = unsafe { raw::mlirOperationGetNumRegions(op) };
        let first = if num_regions > 0 {
            let region = unsafe { raw::mlirOperationGetRegion(op, 0) };
            let block = unsafe { raw::mlirRegionGetFirstBlock(region) };
            if block.ptr.is_null() {
                raw::MlirOperation { ptr: std::ptr::null_mut() }
            } else {
                unsafe { raw::mlirBlockGetFirstOperation(block) }
            }
        } else {
            raw::MlirOperation { ptr: std::ptr::null_mut() }
        };
        OperationChildren {
            current: first,
            started: false,
            _module: std::marker::PhantomData,
        }
    }
}

impl<'m> Iterator for OperationChildren<'m> {
    type Item = Operation<'m>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.started {
            self.current = unsafe { raw::mlirOperationGetNextInBlock(self.current) };
        }
        self.started = true;
        if self.current.ptr.is_null() {
            return None;
        }
        Some(Operation {
            op: self.current,
            _module: std::marker::PhantomData,
        })
    }
}

fn mlir_string_ref(s: &CStr) -> raw::MlirStringRef {
    raw::MlirStringRef {
        data: s.as_ptr(),
        length: s.to_bytes().len(),
    }
}

fn mlir_string_ref_to_string(s: raw::MlirStringRef) -> String {
    if s.data.is_null() || s.length == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(s.data as *const u8, s.length) };
    String::from_utf8_lossy(bytes).into_owned()
}
