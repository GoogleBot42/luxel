use crate::*;
use luxel_core::kinds::Kinds;
use luxel_core::vm::Program;
pub fn compile(_p: &Program, _k: &Kinds, _e: &Env) -> Result<NativeImage, Refusal> { Err(Refusal::Untyped) }
