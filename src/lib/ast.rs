//! Contains the "unresolved" AST which is output by the parser
//!
//! Unresolved meaning it needs to be checked for duplicate
//! definitions, unknown types, etc.
/// Unresolved is an "unchecked" type, which may be an array type
///
/// (identifier, is_array, is_optional)
#[derive(Clone, PartialEq, Debug)]
pub struct Unresolved<'a>(pub &'a str, pub bool, pub bool);
/// Enum is just a list of its variants, which are plain strings
#[derive(Clone, PartialEq, Debug)]
pub struct Enum<'a>(pub Vec<&'a str>);
/// Struct is a list of pairs of `identifier:type`, where `type` may be an array
#[derive(Clone, PartialEq, Debug)]
pub struct Struct<'a>(pub Vec<(&'a str, Unresolved<'a>)>);

#[derive(Clone, PartialEq, Debug)]
pub enum Type<'a> {
    Enum(Enum<'a>),
    Struct(Struct<'a>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Node<'a> {
    Decl(&'a str, Type<'a>),
    Export(&'a str),
}
pub type AST<'a> = Vec<Node<'a>>;
