extern crate chrono;
extern crate peg;
#[macro_use]
extern crate thiserror;

use peg::error::ParseError;
use peg::str::LineCol;

pub mod ast;
pub mod check;
pub mod gen;
pub mod parser;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Parsing failed with:\n{0}")]
    Parse(String),
    #[error("One or more type errors:\n{0}")]
    Check(String),
}

fn pretty_error(file: &str, err: ParseError<LineCol>) -> String {
    let mut out_str = String::new();
    out_str += &format!(
        "\nUnexpected token '{}' at line {}, column {}",
        file.chars().nth(err.location.offset).unwrap_or('\0'),
        err.location.line,
        err.location.column
    );
    let line = file.split('\n').nth(err.location.line - 1).unwrap_or("\0");
    out_str += &format!("\n|\n|  {}\n", line);
    let mark_column = match err.location.column {
        n if n < 1 => 0,
        n => n - 1,
    };
    out_str += &format!("|~~{}^\n", "~".repeat(mark_column));

    out_str
}

pub fn compile<'s, Lang>(schema: &'s str) -> Result<String, Error>
where
    Lang: gen::Language + Default + gen::Common,
    check::Enum<'s>: gen::Definition<Lang>,
    check::Struct<'s>: gen::Definition<Lang>,
    check::Export<'s>: gen::ReadImpl<Lang> + gen::WriteImpl<Lang>,
{
    let ast = match parser::pkt::schema(schema) {
        Ok(ast) => ast,
        Err(e) => return Err(Error::Parse(pretty_error(schema, e))),
    };
    let resolved = match check::type_check(ast) {
        Ok(r) => r,
        Err(e) => return Err(Error::Check(e)),
    };
    Ok(gen::generate::<Lang>(&resolved))
}
