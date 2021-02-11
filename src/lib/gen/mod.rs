pub mod gen_ctx;
pub mod rust;
pub mod ts;

use std::fmt::Write;

use fstrings::format_args_f;
use gen_ctx::GenCtx;

use super::*;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

pub fn generate<'s, Lang>(from: &check::Resolved<'s>) -> String
where
    Lang: Language + Default + Common,
    check::Enum<'s>: Definition<Lang>,
    check::Struct<'s>: Definition<Lang>,
    check::Export<'s>: ReadImpl<Lang> + WriteImpl<Lang>,
{
    let mut gen = Generator::<Lang>::new();
    gen.push_meta();
    gen.push_common();
    for (name, ty) in &from.types {
        let field_type = &*ty.borrow();
        match &field_type.1 {
            // skip builtins, they are defined by push_common()
            check::ResolvedType::Builtin(_) => continue,
            check::ResolvedType::Enum(e) => gen.push_def(name, e),
            check::ResolvedType::Struct(s) => gen.push_def(name, s),
        };
    }
    gen.push_read_impl(from.export.name, &from.export);
    gen.push_write_impl(from.export.name, &from.export);
    gen.finish()
}

#[macro_export]
macro_rules! append {
    ($dst:expr, $($arg:tt)*) => (fstrings::write_f!($dst, $($arg)*).unwrap());

    ($dst:expr, $arg:expr) => (fstrings::write_f!($dst, "{}", $arg).unwrap());
}

#[macro_export]
macro_rules! cat {
    ($ctx:ident +++) => { $ctx.push_indent() };
    ($ctx:ident ---) => { $ctx.pop_indent() };

    ($ctx:ident, $($arg:tt)*) => {{
        let fmt = fstrings::format_f!($($arg)*);
        fstrings::write_f!($ctx.out, "{}{}", $ctx.indentation, fmt).unwrap()
    }};
}

pub trait Language {}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Generator<L: Language + Default + Common> {
    state: L,
    buffer: String,
}

impl<L: Language + Default + Common> Generator<L> {
    pub fn new() -> Self {
        Generator {
            state: L::default(),
            buffer: String::new(),
        }
    }

    /// Empty line
    pub fn push_line(&mut self) {
        append!(&mut self.buffer, "\n");
    }

    /// Version info
    pub fn push_meta(&mut self) {
        append!(
            &mut self.buffer,
            "// Generated by packetc v{} at {}\n",
            VERSION.unwrap_or("???"),
            chrono::Utc::now().to_rfc2822()
        );
    }

    /// Anything that is present in all files of a given language
    pub fn push_common(&mut self) { self.state.gen_common(&mut self.buffer); }

    /// A definition is a struct, interface, etc - anything that defines
    /// the layout of data for a given language
    pub fn push_def(&mut self, name: &str, which: &impl Definition<L>) {
        which.gen_def(&mut self.state, name, &mut self.buffer);
    }

    ///
    pub fn push_write_impl(&mut self, name: &str, which: &impl WriteImpl<L>) {
        which.gen_write_impl(&mut self.state, name, &mut self.buffer);
    }

    pub fn push_read_impl(&mut self, name: &str, which: &impl ReadImpl<L>) {
        which.gen_read_impl(&mut self.state, name, &mut self.buffer);
    }

    pub fn finish(mut self) -> String { std::mem::take(&mut self.buffer) }
}

pub trait Common {
    fn gen_common(&self, out: &mut String);
}

pub trait WriteImpl<K: Language> {
    fn gen_write_impl(&self, state: &mut K, name: &str, out: &mut String);
}

pub trait ReadImpl<K: Language> {
    fn gen_read_impl(&self, state: &mut K, name: &str, out: &mut String);
}

pub trait Definition<K: Language> {
    fn gen_def(&self, state: &mut K, name: &str, out: &mut String);
}
