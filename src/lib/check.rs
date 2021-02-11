//! Contains all the type-checking code
//!
//! Type-checking is done in two passes, so that it's possible to have lexical
//! scoping.
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::{cell::RefCell, fmt, fmt::Display, fmt::Formatter};

use super::*;

// TODO: real error type + report in a nice way

fn get_export<'a>(ast: &[ast::Node<'a>]) -> Result<&'a str, String> {
    let mut export = None;
    for node in ast {
        match node {
            ast::Node::Export(n) if export.is_none() => export = Some(n),
            ast::Node::Export(_) => return Err("Schema has more than one export".to_string()),
            _ => (),
        }
    }
    match export {
        Some(e) => Ok(e),
        None => Err("Schema has no export".to_string()),
    }
}

fn collect_types<'a>(ast: &[ast::Node<'a>]) -> Result<HashMap<&'a str, ast::Type<'a>>, String> {
    let mut cache = HashMap::new();

    for node in ast {
        if let ast::Node::Decl(n, t) = node {
            if cache.contains_key(n) {
                return Err(format!("Schema has duplicate declaration: {}", n));
            }
            cache.insert(*n, t.clone());
        }
    }

    Ok(cache)
}

#[derive(Clone, PartialEq, Debug)]
pub enum Builtin {
    Uint8,
    Uint16,
    Uint32,
    Int8,
    Int16,
    Int32,
    Float,
    String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EnumVariant<'a> {
    pub name: &'a str,
    pub value: usize,
}
#[derive(Clone, PartialEq, Debug)]
pub enum EnumRepr {
    U8,
    U16,
    U32,
}
impl Display for EnumRepr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EnumRepr::U8 => write!(f, "u8"),
            EnumRepr::U16 => write!(f, "u16"),
            EnumRepr::U32 => write!(f, "u32"),
        }
    }
}
#[derive(Clone, PartialEq, Debug)]
pub struct Enum<'a> {
    pub repr: EnumRepr,
    pub variants: Vec<EnumVariant<'a>>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct StructField<'a> {
    pub name: &'a str,
    pub r#type: Ptr<(&'a str, ResolvedType<'a>)>,
    pub array: bool,
    pub optional: bool,
}
#[derive(Clone, PartialEq, Debug)]
pub struct Struct<'a> {
    pub fields: Vec<StructField<'a>>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ResolvedType<'a> {
    Builtin(Builtin),
    Enum(Enum<'a>),
    Struct(Struct<'a>),
}

impl<'a> ResolvedType<'a> {
    fn get_struct_variant(&self) -> Option<Struct<'a>> {
        match self {
            ResolvedType::Struct(s) => Some(s.clone()),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Ptr<T>(pub Rc<RefCell<T>>);
impl<T> Ptr<T> {
    pub fn new(value: T) -> Ptr<T> { Ptr(Rc::new(RefCell::new(value))) }
    pub fn strong_count(&self) -> usize { Rc::strong_count(&self.0) }
}
impl<T> std::ops::Deref for Ptr<T> {
    type Target = Rc<RefCell<T>>;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Export<'a> {
    pub name: &'a str,
    pub r#struct: Struct<'a>,
}

fn get_builtins<'a>() -> HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>> {
    vec![
        ("uint8", Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8)))),
        ("uint16", Ptr::new(("uint16", ResolvedType::Builtin(Builtin::Uint16)))),
        ("uint32", Ptr::new(("uint32", ResolvedType::Builtin(Builtin::Uint32)))),
        ("int8", Ptr::new(("int8", ResolvedType::Builtin(Builtin::Int8)))),
        ("int16", Ptr::new(("int16", ResolvedType::Builtin(Builtin::Int16)))),
        ("int32", Ptr::new(("int32", ResolvedType::Builtin(Builtin::Int32)))),
        ("float", Ptr::new(("float", ResolvedType::Builtin(Builtin::Float)))),
        ("string", Ptr::new(("string", ResolvedType::Builtin(Builtin::String)))),
    ]
    .into_iter()
    .collect()
}

fn resolve_struct_field<'a>(
    fname: &'a str,
    fty: ast::Unresolved<'a>,
    resolved: &HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    ttypename: &'a str,
) -> Result<Option<StructField<'a>>, String> {
    match resolved.get(&fty.0) {
        Some(rty) => {
            if fty.1 && fty.2 {
                return Err(format!(
                    "Field '{}' in struct '{}' cannot be optional and array at once",
                    fname, ttypename
                ));
            }
            Ok(Some(StructField {
                name: fname,
                r#type: rty.clone(),
                array: fty.1,
                optional: fty.2,
            }))
        }
        None => Ok(None),
    }
}

fn resolve_enum<'a>(name: &'a str, ty: ast::Enum<'a>) -> Result<(EnumRepr, Vec<EnumVariant<'a>>), String> {
    // find the smallest possible representation for this enum
    let repr = match ty.0.len() {
        n if n == 0 => return Err(format!("Enum '{}' must have at least one variant", name)),
        n if n <= 8 => EnumRepr::U8,
        n if n <= 16 => EnumRepr::U16,
        n if n <= 32 => EnumRepr::U32,
        n => return Err(format!("Enum '{}' has too many variants ({}/32)", name, n)),
    };
    // resolve the variants by assigning each one to a single bit
    let mut variant_names = HashSet::new();
    let mut count = 0usize;
    let mut variants = Vec::with_capacity(ty.0.len());
    for variant in ty.0.into_iter() {
        if variant_names.contains(&variant) {
            return Err(format!("Duplicate variant '{}' on enum '{}'", variant, name));
        }
        variant_names.insert(variant);
        variants.push(EnumVariant {
            name: variant,
            value: {
                count += 1;
                count - 1
            },
        });
    }
    Ok((repr, variants))
}

fn resolve_one_first_pass<'a>(
    name: &'a str,
    ty: ast::Type<'a>,
    builtins: &HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    first_pass: &mut HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    unresolved: &mut HashMap<&'a str, ast::Type<'a>>,
) -> Result<(), String> {
    match ty {
        ast::Type::Enum(e) => {
            match resolve_enum(name, e) {
                Ok(rty) => {
                    unresolved.remove(name);
                    first_pass.insert(
                        name,
                        Ptr::new((
                            name,
                            ResolvedType::Enum(Enum {
                                repr: rty.0,
                                variants: rty.1,
                            }),
                        )),
                    );
                }
                Err(err) => return Err(err),
            };
        }
        ast::Type::Struct(s) => {
            let mut field_names = HashSet::new();
            let mut fields = Vec::new();
            for (fname, fty) in s.0.iter() {
                if field_names.contains(&fname) {
                    return Err(format!("Duplicate field '{}' on struct '{}'", fname, name));
                }
                field_names.insert(fname);
                if let Some(field) = resolve_struct_field(fname, fty.clone(), builtins, name)? {
                    fields.push(field);
                } else {
                    break;
                }
            }
            if fields.len() == s.0.len() {
                unresolved.remove(&name);
                first_pass.insert(name, Ptr::new((name, ResolvedType::Struct(Struct { fields }))));
            }
        }
    }
    Ok(())
}

// This should consume the AST and return a type-checked version
fn resolve_first_pass<'a>(
    ast: ast::AST<'a>,
    builtins: &HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    first_pass: &mut HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    unresolved: &mut HashMap<&'a str, ast::Type<'a>>,
) -> Result<(), String> {
    for node in ast {
        if let ast::Node::Decl(name, ty) = node {
            if let Err(err) = resolve_one_first_pass(name, ty, builtins, first_pass, unresolved) {
                return Err(err);
            }
        }
    }
    Ok(())
}

fn resolve_one_second_pass<'a>(
    name: &'a str,
    ty: ast::Type<'a>,
    cache: &mut HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    visited: &mut HashSet<&'a str>,
    unresolved: &mut HashMap<&'a str, ast::Type<'a>>,
) -> Result<(), String> {
    // if it's already resolved, dont resolve again
    if cache.contains_key(&name) {
        return Ok(());
    }
    // otherwise try to resolve it
    if let ast::Type::Struct(s) = ty {
        // if we've already visited this type, it's a cycle
        if visited.contains(&name) {
            return Err(format!(
                "Found a cycle between two or more top level definitions in type '{}'",
                &name
            ));
        }
        visited.insert(name);
        // iterate over each field, trying to resolve it
        // store any field (+ its type) which could not be resolved
        let mut not_resolved = Vec::new();
        let mut fields = Vec::new();
        for (field_name, field_type) in s.0.into_iter() {
            if let Some(field) = resolve_struct_field(field_name, field_type.clone(), cache, name)? {
                fields.push(field);
            } else {
                not_resolved.push((field_name, field_type));
            }
        }
        if not_resolved.is_empty() {
            // if all the fields are resolved, construct the type and cache it
            cache.insert(name, Ptr::new((name, ResolvedType::Struct(Struct { fields }))));
        } else {
            // otherwise, for each field that couldn't be resolved, try to resolve it
            for (_, field_type) in not_resolved.iter() {
                let ftype_name = field_type.0;
                // try to find the field's typename in whatever is left unresolved
                if let Some(utype) = unresolved.remove(&ftype_name) {
                    // if it exists, try to resolve it by recursively calling
                    // the function we're in

                    // it may fail, so propagate the error out
                    resolve_one_second_pass(ftype_name, utype, cache, visited, unresolved)?;
                } else if !cache.contains_key(&ftype_name) {
                    //  if the field's typename is unresolved and not in the cache (resolved), it
                    // doesn't exist.
                    return Err(format!("Declaration for type '{}' does not exist", ftype_name));
                }
            }
            // if we get here, it means all the field's types were successfully resolved and
            // placed in the cache so finish resolving our fields
            let mut now_resolved = Vec::new();
            for (fname, fty) in not_resolved.into_iter() {
                now_resolved.push(resolve_struct_field(fname, fty, cache, name)?.unwrap());
            }
            // and we have a complete type
            cache.insert(
                name,
                Ptr::new((
                    name,
                    ResolvedType::Struct(Struct {
                        fields: fields.into_iter().chain(now_resolved).collect(),
                    }),
                )),
            );
        }
    } else {
        panic!(format!(
            "Something unresolved which is not a struct got into the second pass: {:#?}",
            ty
        ));
    }
    Ok(())
}

fn resolve_second_pass<'a>(
    cache: &mut HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
    mut unresolved: HashMap<&'a str, ast::Type<'a>>,
) -> Result<(), String> {
    let mut visited = HashSet::new();
    for (name, ty) in unresolved.clone() {
        if let Err(err) = resolve_one_second_pass(name, ty, cache, &mut visited, &mut unresolved) {
            return Err(err);
        }
    }
    Ok(())
}

fn collect_used_types<'a>(visited: &mut HashSet<&'a str>, ty: &(&'a str, ResolvedType<'a>)) {
    visited.insert(ty.0);
    if let Some(ty) = ty.1.get_struct_variant() {
        for field in ty.fields.iter() {
            let field = &*field.r#type.borrow();
            if visited.contains(&field.0) {
                continue;
            }
            collect_used_types(visited, field);
        }
    }
}

fn remove_unused<'a>(visited: HashSet<&'a str>, resolved: &mut HashMap<&'a str, Ptr<(&'a str, ResolvedType)>>) {
    // TODO: print a warning (if configured) for each unused type
    resolved.retain(|name, _| visited.contains(name));
}

fn resolve_export<'a>(
    name: &'a str,
    resolved: &mut HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
) -> Result<Export<'a>, String> {
    if let Some(export) = resolved.get(&name).cloned() {
        if let Some(ty) = export.borrow().1.get_struct_variant() {
            // Use this opportunity to discard unused types.
            let mut visited = [name].iter().copied().collect();
            for field in ty.fields.iter() {
                collect_used_types(&mut visited, &*field.r#type.borrow());
            }
            remove_unused(visited, resolved);

            Ok(Export { name, r#struct: ty })
        } else {
            Err(format!("Attempted to export '{}', which is not a struct", name))
        }
    } else {
        Err(format!("Export '{}' could not be resolved", name))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Resolved<'a> {
    pub export: Export<'a>,
    pub types: HashMap<&'a str, Ptr<(&'a str, ResolvedType<'a>)>>,
}

pub fn type_check(ast: ast::AST<'_>) -> Result<Resolved<'_>, String> {
    let export = match get_export(&ast) {
        Ok(e) => e,
        Err(e) => return Err(e),
    };
    let mut unresolved = match collect_types(&ast) {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    // pre-pass: collect builtins
    let cache = get_builtins();
    // first pass: collect enums + structs with only builtins as field types
    let mut first_pass = HashMap::new();
    if let Err(err) = resolve_first_pass(ast, &cache, &mut first_pass, &mut unresolved) {
        return Err(err);
    };
    // second pass: collect structs with other structs (made up of builtins) as
    // field types
    let mut cache = cache.into_iter().chain(first_pass).collect::<HashMap<_, _>>();
    if let Err(err) = resolve_second_pass(&mut cache, unresolved) {
        return Err(err);
    };
    // export pass: collect the resolved type we're exporting
    let export = match resolve_export(export, &mut cache) {
        Ok(e) => e,
        Err(err) => return Err(err),
    };
    Ok(Resolved { export, types: cache })
}

#[cfg(test)]
mod tests {
    use std::vec;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn type_check_passes() {
        // check if a valid AST containing all language features passes the type check
        use ast::*;
        let test: AST = vec![
            Node::Decl("Flag", Type::Enum(Enum(vec!["A", "B"]))),
            Node::Decl(
                "Position",
                Type::Struct(Struct(vec![
                    ("x", Unresolved("float", false, false)),
                    ("y", Unresolved("float", false, false)),
                ])),
            ),
            Node::Decl(
                "Value",
                Type::Struct(Struct(vec![
                    ("a", Unresolved("uint32", false, false)),
                    ("b", Unresolved("int32", false, false)),
                    ("c", Unresolved("uint8", false, false)),
                    ("d", Unresolved("uint8", false, false)),
                ])),
            ),
            Node::Decl(
                "ComplexType",
                Type::Struct(Struct(vec![
                    ("flag", Unresolved("Flag", false, false)),
                    ("pos", Unresolved("Position", false, false)),
                    ("names", Unresolved("string", true, false)),
                    ("values", Unresolved("Value", true, false)),
                ])),
            ),
            Node::Export("ComplexType"),
        ];
        // TODO: check equality of Resolved AST instead of checking if this is an error
        // Rc<T> == Rc<T> if T == T, according to https://doc.rust-lang.org/src/alloc/rc.rs.html#1325
        type_check(test).unwrap();
    }

    #[test]
    fn empty_enum() {
        // an enum must have at least one variant
        use ast::*;
        let test: AST = vec![
            Node::Decl("Flag", Type::Enum(Enum(vec![]))),
            Node::Decl(
                "Test",
                Type::Struct(Struct(vec![("flag", Unresolved("Flag", false, false))])),
            ),
            Node::Export("Test"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Enum 'Flag' must have at least one variant"
        );
    }

    #[test]
    fn optional_and_array() {
        // field cannot be optional and array at once
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Test",
                Type::Struct(Struct(vec![("a", Unresolved("uint8", true, true))])),
            ),
            Node::Export("Test"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Field 'a' in struct 'Test' cannot be optional and array at once"
        );
    }

    #[test]
    fn too_many_exports() {
        // a schema file may have only one export
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Position",
                Type::Struct(Struct(vec![
                    ("x", Unresolved("float", false, false)),
                    ("y", Unresolved("float", false, false)),
                ])),
            ),
            Node::Export("Position"),
            Node::Export("Position"),
        ];
        assert_eq!(type_check(test).unwrap_err(), "Schema has more than one export");
    }

    #[test]
    fn no_export() {
        // a schema file must export something
        use ast::*;
        let test: AST = vec![Node::Decl(
            "Position",
            Type::Struct(Struct(vec![
                ("x", Unresolved("float", false, false)),
                ("y", Unresolved("float", false, false)),
            ])),
        )];
        assert_eq!(type_check(test).unwrap_err(), "Schema has no export");
    }

    #[test]
    fn duplicate_enum_variants() {
        use ast::*;
        let test: AST = vec![
            Node::Decl("Flag", Type::Enum(Enum(vec!["A", "A"]))),
            Node::Decl(
                "Test",
                Type::Struct(Struct(vec![("flag", Unresolved("Flag", false, false))])),
            ),
            Node::Export("Test"),
        ];
        assert_eq!(type_check(test).unwrap_err(), "Duplicate variant 'A' on enum 'Flag'");
    }

    #[test]
    fn duplicate_field_name() {
        // a struct field must have a unique name
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Position",
                Type::Struct(Struct(vec![
                    ("x", Unresolved("float", false, false)),
                    ("x", Unresolved("float", false, false)),
                ])),
            ),
            Node::Export("Position"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Duplicate field 'x' on struct 'Position'"
        );
    }

    #[test]
    fn duplicate_declaration() {
        // a top-level declaration must have a unique name
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Position",
                Type::Struct(Struct(vec![
                    ("x", Unresolved("float", false, false)),
                    ("y", Unresolved("float", false, false)),
                ])),
            ),
            Node::Decl(
                "Position",
                Type::Struct(Struct(vec![
                    ("x", Unresolved("float", false, false)),
                    ("y", Unresolved("float", false, false)),
                ])),
            ),
            Node::Export("Position"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Schema has duplicate declaration: Position"
        );
    }

    #[test]
    fn too_many_enum_variants() {
        // because each variant is assigned to a bit, and
        // the maximum representation size is 32 bit,
        // an enum may have at most 32 variants
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Flag",
                Type::Enum(Enum(vec![
                    "A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11", "A12", "A13", "A14",
                    "A15", "A16", "A17", "A18", "A19", "A20", "A21", "A22", "A23", "A24", "A25", "A26", "A27", "A28",
                    "A29", "A30", "A31", // one too many
                    "A32",
                ])),
            ),
            Node::Decl(
                "Test",
                Type::Struct(Struct(vec![("flag", Unresolved("Flag", false, false))])),
            ),
            Node::Export("Test"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Enum 'Flag' has too many variants (33/32)"
        );
    }

    #[test]
    fn could_not_resolve_unknown() {
        // an unresolvable type is either non-existant, nested, or recursive type
        // in this case the type that's being resolved doesn't exist
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Test",
                Type::Struct(Struct(vec![("flag", Unresolved("Flag", false, false))])),
            ),
            Node::Export("Test"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Declaration for type 'Flag' does not exist"
        );
    }

    #[test]
    fn duplicate_type_uses_are_resolved() {
        use ast::*;
        let test = vec![
            Node::Decl(
                "A",
                Type::Struct(Struct(vec![("b", Unresolved("int32", false, false))])),
            ),
            Node::Decl("B", Type::Struct(Struct(vec![("a", Unresolved("A", false, false))]))),
            Node::Decl(
                "D",
                Type::Struct(Struct(vec![
                    ("b1", Unresolved("B", false, false)),
                    ("b2", Unresolved("B", false, false)),
                ])),
            ),
            Node::Export("D"),
        ];
        assert!(type_check(test).is_ok());
    }

    #[test]
    fn discards_unused() {
        // any types which aren't used are silently discarded,
        // so that they don't clutter the final generated file.
        let test: ast::AST = {
            use ast::*;
            vec![
                Node::Decl(
                    "UnusedType",
                    Type::Struct(Struct(vec![("test", Unresolved("uint8", false, false))])),
                ),
                Node::Decl("Flag", Type::Enum(Enum(vec!["A", "B"]))),
                Node::Decl(
                    "Test",
                    Type::Struct(Struct(vec![("flag", Unresolved("Flag", false, false))])),
                ),
                Node::Export("Test"),
            ]
        };
        let checked = type_check(test).unwrap();
        assert!(!checked.types.contains_key("UnusedType"));
    }

    #[test]
    fn resolve_nested() {
        // the type that's being resolved is too deeply nested
        use ast::*;
        let test: AST = vec![
            Node::Decl("Flag", Type::Enum(Enum(vec!["A", "B"]))),
            Node::Decl(
                "TestA",
                Type::Struct(Struct(vec![("test", Unresolved("Flag", false, false))])),
            ),
            Node::Decl(
                "TestB",
                Type::Struct(Struct(vec![("test", Unresolved("TestA", false, false))])),
            ),
            Node::Decl(
                "TestC",
                Type::Struct(Struct(vec![("test", Unresolved("TestB", false, false))])),
            ),
            Node::Export("TestC"),
        ];
        type_check(test).unwrap();
    }

    #[test]
    fn could_not_resolve_recursive() {
        // the type that's being resolved references itself
        use ast::*;
        let test: AST = vec![
            Node::Decl(
                "Test",
                Type::Struct(Struct(vec![("test", Unresolved("Test", false, false))])),
            ),
            Node::Export("Test"),
        ];
        let actual = type_check(test);
        assert_eq!(
            actual.unwrap_err(),
            "Found a cycle between two or more top level definitions in type 'Test'"
        );
    }

    #[test]
    fn could_not_resolve_export() {
        // the type does not exist
        use ast::*;
        let test: AST = vec![Node::Export("Test")];
        assert_eq!(type_check(test).unwrap_err(), "Export 'Test' could not be resolved");
    }

    #[test]
    fn export_only_struct() {
        // only structs may be exported
        use ast::*;
        let test: AST = vec![
            Node::Decl("Flag", Type::Enum(Enum(vec!["A", "B"]))),
            Node::Export("Flag"),
        ];
        assert_eq!(
            type_check(test).unwrap_err(),
            "Attempted to export 'Flag', which is not a struct"
        );
    }
}
