use std::collections::HashSet;

use fstrings::{format_args_f, format_f};

use super::*;

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Rust {
    imports: HashSet<String>,
}
impl Language for Rust {}

impl Common for Rust {
    fn gen_common(&self, out: &mut String) {
        let ctx = GenCtx::new(out);
        cat!(
            ctx,
            "#![allow(dead_code, non_camel_case_types, unused_imports, clippy::field_reassign_with_default)]\n"
        );
        cat!(ctx, "use std::convert::TryFrom;\n");
    }
}

fn varname(stack: &[String], name: &str) -> String { format!("{}_{}", stack.join("_"), name) }
fn bindname(stack: &[String]) -> String { stack.join("_") }
fn fname(stack: &[String]) -> String { stack.join(".") }

fn gen_write_impl_optional(ctx: &mut GenCtx, by_ref: bool, body: impl Fn(&mut GenCtx)) {
    let fname = fname(&ctx.stack);
    let bind_var = bindname(&ctx.stack);
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(bind_var.clone());

    let ref_prefix = if by_ref { "&" } else { "" };
    cat!(ctx, "match {ref_prefix}{fname} {{\n");
    cat!(ctx +++);
    cat!(ctx, "None => writer.write_uint8(0u8),\n");
    cat!(ctx, "Some({bind_var}) => {{\n");
    cat!(ctx +++);
    cat!(ctx, "writer.write_uint8(1u8);\n");

    body(ctx);

    cat!(ctx ---);
    cat!(ctx, "}}\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_write_impl_array(ctx: &mut GenCtx, body: impl Fn(&mut GenCtx)) {
    let fname = fname(&ctx.stack);
    let item_var = varname(&ctx.stack, "item");
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(item_var.clone());

    cat!(ctx, "writer.write_uint32({fname}.len() as u32);\n");
    cat!(ctx, "for {item_var} in {fname}.iter() {{\n");
    cat!(ctx +++);

    body(ctx);

    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_write_impl_builtin(ctx: &mut GenCtx, type_info: &check::Builtin, type_name: &str) {
    let fname = fname(&ctx.stack);
    match type_info {
        check::Builtin::String => {
            cat!(ctx, "writer.write_uint32({fname}.len() as u32);\n");
            cat!(ctx, "writer.write_string(&{fname});\n");
        }
        _ => cat!(ctx, "writer.write_{type_name}({fname});\n"),
    }
}

fn gen_write_impl_enum(ctx: &mut GenCtx, type_info: &check::Enum, _: &str) {
    let repr_name = match &type_info.repr {
        check::EnumRepr::U8 => "uint8",
        check::EnumRepr::U16 => "uint16",
        check::EnumRepr::U32 => "uint32",
    };
    let fname = fname(&ctx.stack);
    cat!(ctx, "writer.write_{repr_name}({fname} as {type_info.repr});\n");
}

fn gen_write_impl_struct(ctx: &mut GenCtx, ty: &check::Struct, _: &str) {
    for f in &ty.fields {
        ctx.push_fname(f.name);
        let fty = &*f.r#type.borrow();

        use check::ResolvedType::*;
        let mut by_ref = false;
        let mut generator: Box<dyn Fn(&mut GenCtx)> = match &fty.1 {
            Builtin(fty_info) => Box::new(move |ctx| gen_write_impl_builtin(ctx, &fty_info, &fty.0)),
            Enum(fty_info) => Box::new(move |ctx| gen_write_impl_enum(ctx, &fty_info, &fty.0)),
            Struct(fty_info) => {
                by_ref = true;
                Box::new(move |ctx| gen_write_impl_struct(ctx, &fty_info, &fty.0))
            }
        };
        if f.array {
            generator = Box::new(move |ctx| gen_write_impl_array(ctx, |ctx| generator(ctx)))
        }
        if f.optional {
            generator = Box::new(move |ctx| gen_write_impl_optional(ctx, by_ref, |ctx| generator(ctx)))
        }
        generator(ctx);

        ctx.pop_fname();
    }
}

impl<'a> WriteImpl<Rust> for check::Export<'a> {
    fn gen_write_impl(&self, _: &mut Rust, name: &str, out: &mut String) {
        let mut ctx = GenCtx::new(out);
        ctx.push_fname("input");
        cat!(
            ctx,
            "pub fn write(writer: &mut packet::writer::Writer, input: &{name}) {{\n"
        );
        cat!(ctx +++);
        gen_write_impl_struct(&mut ctx, &self.r#struct, &name);
        cat!(ctx ---);
        cat!(ctx, "}}\n");
    }
}

fn gen_read_impl_optional(ctx: &mut GenCtx, type_name: &str, body: impl Fn(&mut GenCtx)) {
    let fname = self::fname(&ctx.stack);
    let bind_var = bindname(&ctx.stack);
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(bind_var.clone());

    cat!(ctx, "if reader.read_uint8()? > 0 {{\n");
    cat!(ctx +++);
    cat!(ctx, "let mut {bind_var} = {type_name}::default();\n");

    body(ctx);

    cat!(ctx, "{fname} = Some({bind_var});\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_read_impl_array(ctx: &mut GenCtx, type_name: &str, body: impl Fn(&mut GenCtx)) {
    let len_var = varname(&ctx.stack, "len");
    let fname = fname(&ctx.stack);
    let item_var = varname(&ctx.stack, "item");
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(item_var.clone());

    cat!(ctx, "let {len_var} = reader.read_uint32()? as usize;\n");
    cat!(ctx, "{fname}.reserve({len_var});\n");
    cat!(ctx, "for _ in 0..{len_var} {{\n");
    cat!(ctx +++);
    cat!(ctx, "let mut {item_var} = {type_name}::default();\n");

    body(ctx);

    cat!(ctx, "{fname}.push({item_var});\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_read_impl_builtin(ctx: &mut GenCtx, type_info: &check::Builtin, type_name: &str) {
    let fname = fname(&ctx.stack);
    match type_info {
        check::Builtin::String => {
            let len_var = varname(&ctx.stack, "len");
            cat!(ctx, "let {len_var} = reader.read_uint32()? as usize;\n");
            cat!(ctx, "{fname} = reader.read_string({len_var})?;\n");
        }
        _ => {
            cat!(ctx, "{fname} = reader.read_{type_name}()?;\n")
        }
    }
}

fn gen_read_impl_enum(ctx: &mut GenCtx, type_info: &check::Enum, type_name: &str) {
    let repr_name = match type_info.repr {
        check::EnumRepr::U8 => "uint8",
        check::EnumRepr::U16 => "uint16",
        check::EnumRepr::U32 => "uint32",
    };
    let fname = fname(&ctx.stack);
    cat!(ctx, "{fname} = {type_name}::try_from(reader.read_{repr_name}()?)?;\n");
}

fn resolve_typename(type_name: &str) -> Option<&'static str> {
    match type_name {
        "uint8" => Some("u8"),
        "uint16" => Some("u16"),
        "uint32" => Some("u32"),
        "int8" => Some("i8"),
        "int16" => Some("i16"),
        "int32" => Some("i32"),
        "float" => Some("f32"),
        "string" => Some("String"),
        _ => None,
    }
}

fn gen_read_impl_struct(ctx: &mut GenCtx, ty: &check::Struct, _name: &str) {
    for f in &ty.fields {
        ctx.push_fname(f.name);
        let fty = &*f.r#type.borrow();

        use check::ResolvedType::*;
        let tyname = resolve_typename(fty.0).unwrap_or(fty.0);
        let mut generator: Box<dyn Fn(&mut GenCtx)> = match &fty.1 {
            Builtin(fty_info) => Box::new(move |ctx| gen_read_impl_builtin(ctx, &fty_info, fty.0)),
            Enum(fty_info) => Box::new(move |ctx| gen_read_impl_enum(ctx, &fty_info, fty.0)),
            Struct(fty_info) => Box::new(move |ctx| gen_read_impl_struct(ctx, &fty_info, fty.0)),
        };
        if f.array {
            generator = Box::new(move |ctx| gen_read_impl_array(ctx, tyname, |ctx| generator(ctx)))
        }
        if f.optional {
            generator = Box::new(move |ctx| gen_read_impl_optional(ctx, tyname, |ctx| generator(ctx)))
        }
        generator(ctx);
        ctx.pop_fname();
    }
}

impl<'a> ReadImpl<Rust> for check::Export<'a> {
    fn gen_read_impl(&self, _: &mut Rust, name: &str, out: &mut String) {
        let mut ctx = GenCtx::new(out);
        ctx.push_fname("output");
        cat!(
            ctx,
            "pub fn read(reader: &mut packet::reader::Reader, output: &mut {name}) -> Result<(), packet::Error> {{\n"
        );
        cat!(ctx +++);
        gen_read_impl_struct(&mut ctx, &self.r#struct, &name);
        cat!(ctx, "Ok(())\n");
        cat!(ctx ---);
        cat!(ctx, "}}\n");
    }
}

fn struct_field_typename(base: &str, array: bool, optional: bool) -> String {
    format_f!(
        "{preopt}{prearr}{base}{postarr}{postopt}",
        preopt = if optional { "Option<" } else { "" },
        prearr = if array { "Vec<" } else { "" },
        postarr = if array { ">" } else { "" },
        postopt = if optional { ">" } else { "" }
    )
}

impl<'a> Definition<Rust> for check::Struct<'a> {
    fn gen_def(&self, _: &mut Rust, name: &str, out: &mut String) {
        let mut ctx = GenCtx::new(out);

        cat!(ctx, "#[derive(Clone, PartialEq, Debug, Default)]\n");
        cat!(ctx, "pub struct {name} {{\n");
        cat!(ctx +++);
        for field in self.fields.iter() {
            let type_info = &*field.r#type.borrow();
            let mut typename: &str = &type_info.0;
            if let check::ResolvedType::Builtin(b) = &type_info.1 {
                typename = match b {
                    check::Builtin::Uint8 => "u8",
                    check::Builtin::Uint16 => "u16",
                    check::Builtin::Uint32 => "u32",
                    check::Builtin::Int8 => "i8",
                    check::Builtin::Int16 => "i16",
                    check::Builtin::Int32 => "i32",
                    check::Builtin::Float => "f32",
                    check::Builtin::String => "String",
                };
            }
            let sftyname = struct_field_typename(typename, field.array, field.optional);
            cat!(ctx, "pub {field.name}: {sftyname},\n");
        }
        cat!(ctx ---);
        cat!(ctx, "}}\n");
    }
}

fn gen_def_enum_default_impl<'a>(ctx: &mut GenCtx, name: &str, ty: &check::Enum<'a>) {
    let first_variant = ty.variants.first().unwrap().name;

    cat!(ctx, "impl Default for {name} {{\n");
    cat!(ctx +++);
    cat!(ctx, "fn default() -> Self {{\n");
    cat!(ctx +++);
    cat!(ctx, "{name}::{first_variant}\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");
}

fn gen_def_enum_tryfrom_impl<'a>(ctx: &mut GenCtx, name: &str, ty: &check::Enum<'a>) {
    cat!(ctx, "impl std::convert::TryFrom<{ty.repr}> for {name} {{\n");
    cat!(ctx +++);
    cat!(ctx, "type Error = packet::Error;\n");
    cat!(ctx, "fn try_from(value: {ty.repr}) -> Result<Self, Self::Error> {{\n");
    cat!(ctx +++);
    cat!(ctx, "match value {{\n");
    cat!(ctx +++);
    for variant in &ty.variants {
        let value = 1 << variant.value;
        cat!(ctx, "{value} => Ok({name}::{variant.name}),\n");
    }
    cat!(
        ctx,
        "_ => Err(packet::Error::InvalidEnumValue(value as usize, \"{name}\"))\n"
    );
    cat!(ctx ---);
    cat!(ctx, "}}\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");
}

impl<'a> Definition<Rust> for check::Enum<'a> {
    fn gen_def(&self, _: &mut Rust, name: &str, out: &mut String) {
        let mut ctx = GenCtx::new(out);

        let repr = &self.repr;

        cat!(ctx, "#[derive(Clone, Copy, PartialEq, Debug)]\n");
        cat!(ctx, "#[repr({repr})]\n");
        cat!(ctx, "pub enum {name} {{\n");
        cat!(ctx +++);
        for variant in self.variants.iter() {
            cat!(ctx, "{variant.name} = 1 << {variant.value},\n");
        }
        cat!(ctx ---);
        cat!(ctx, "}}\n");
        gen_def_enum_default_impl(&mut ctx, name, &self);
        gen_def_enum_tryfrom_impl(&mut ctx, name, &self);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn commmon_gen() {
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_common();
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
#![allow(dead_code, non_camel_case_types, unused_imports, clippy::field_reassign_with_default)]
use std::convert::TryFrom;
"
        );
    }

    #[test]
    fn simple_struct_gen() {
        use check::*;
        let position = Struct {
            fields: vec![
                StructField {
                    name: "x",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
                StructField {
                    name: "y",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
            ],
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_def("Position", &position);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}
"
        );
    }

    #[test]
    fn struct_with_optional_gen() {
        use check::*;
        let test = Struct {
            fields: vec![
                StructField {
                    name: "a",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: true,
                },
                StructField {
                    name: "b",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: true,
                    optional: true,
                },
                StructField {
                    name: "c",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
            ],
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_def("Test", &test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Test {
    pub a: Option<f32>,
    pub b: Option<Vec<f32>>,
    pub c: f32,
}
"
        );
    }

    #[test]
    fn enum_gen() {
        use check::*;
        let flag = Enum {
            repr: EnumRepr::U8,
            variants: vec![EnumVariant { name: "A", value: 0 }, EnumVariant { name: "B", value: 1 }],
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_def("Flag", &flag);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum Flag {
    A = 1 << 0,
    B = 1 << 1,
}
impl Default for Flag {
    fn default() -> Self {
        Flag::A
    }
}
impl std::convert::TryFrom<u8> for Flag {
    type Error = packet::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Flag::A),
            2 => Ok(Flag::B),
            _ => Err(packet::Error::InvalidEnumValue(value as usize, \"Flag\"))
        }
    }
}
"
        );
    }

    #[test]
    fn complex_struct_gen() {
        use check::*;
        let test = Export {
            name: "Test",
            r#struct: Struct {
                fields: vec![
                    StructField {
                        name: "builtin_scalar",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "builtin_array",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "string_scalar",
                        r#type: Ptr::new(("string", ResolvedType::Builtin(Builtin::String))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "string_array",
                        r#type: Ptr::new(("string", ResolvedType::Builtin(Builtin::String))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "enum_scalar",
                        r#type: Ptr::new((
                            "Flag",
                            ResolvedType::Enum(Enum {
                                repr: EnumRepr::U8,
                                variants: vec![],
                            }),
                        )),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "enum_array",
                        r#type: Ptr::new((
                            "Flag",
                            ResolvedType::Enum(Enum {
                                repr: EnumRepr::U8,
                                variants: vec![],
                            }),
                        )),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "struct_scalar",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(Struct { fields: vec![] }))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "struct_array",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(Struct { fields: vec![] }))),
                        array: true,
                        optional: false,
                    },
                ],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_def("Test", &test.r#struct);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Test {
    pub builtin_scalar: u8,
    pub builtin_array: Vec<u8>,
    pub string_scalar: String,
    pub string_array: Vec<String>,
    pub enum_scalar: Flag,
    pub enum_array: Vec<Flag>,
    pub struct_scalar: Position,
    pub struct_array: Vec<Position>,
}
"
        );
    }

    #[test]
    fn optional_write_gen() {
        use check::*;
        let test = Export {
            name: "Test",
            r#struct: Struct {
                fields: vec![
                    StructField {
                        name: "a",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: true,
                    },
                    StructField {
                        name: "b",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: true,
                        optional: true,
                    },
                    StructField {
                        name: "c",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: false,
                    },
                ],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_write_impl("Test", &test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn write(writer: &mut packet::writer::Writer, input: &Test) {
    match input.a {
        None => writer.write_uint8(0u8),
        Some(input_a) => {
            writer.write_uint8(1u8);
            writer.write_uint8(input_a);
        }
    }
    match input.b {
        None => writer.write_uint8(0u8),
        Some(input_b) => {
            writer.write_uint8(1u8);
            writer.write_uint32(input_b.len() as u32);
            for input_b_item in input_b.iter() {
                writer.write_uint8(input_b_item);
            }
        }
    }
    writer.write_uint8(input.c);
}
"
        );
    }

    #[test]
    fn optional_read_gen() {
        use check::*;
        let test = Export {
            name: "Test",
            r#struct: Struct {
                fields: vec![
                    StructField {
                        name: "a",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: true,
                    },
                    StructField {
                        name: "b",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: false,
                    },
                ],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_read_impl("Test", &test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn read(reader: &mut packet::reader::Reader, output: &mut Test) -> Result<(), packet::Error> {
    if reader.read_uint8()? > 0 {
        let mut output_a = u8::default();
        output_a = reader.read_uint8()?;
        output.a = Some(output_a);
    }
    output.b = reader.read_uint8()?;
    Ok(())
}
"
        );
    }

    #[test]
    fn nested_soa_write_gen() {
        use check::*;
        let test_a = Struct {
            fields: vec![
                StructField {
                    name: "first",
                    r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                    array: true,
                    optional: false,
                },
                StructField {
                    name: "second",
                    r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                    array: true,
                    optional: false,
                },
            ],
        };
        let test_b = Export {
            name: "TestB",
            r#struct: Struct {
                fields: vec![StructField {
                    name: "test_a",
                    r#type: Ptr::new(("TestA", ResolvedType::Struct(test_a))),
                    array: true,
                    optional: false,
                }],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_write_impl("TestB", &test_b);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn write(writer: &mut packet::writer::Writer, input: &TestB) {
    writer.write_uint32(input.test_a.len() as u32);
    for input_test_a_item in input.test_a.iter() {
        writer.write_uint32(input_test_a_item.first.len() as u32);
        for input_test_a_item_first_item in input_test_a_item.first.iter() {
            writer.write_uint8(input_test_a_item_first_item);
        }
        writer.write_uint32(input_test_a_item.second.len() as u32);
        for input_test_a_item_second_item in input_test_a_item.second.iter() {
            writer.write_uint8(input_test_a_item_second_item);
        }
    }
}
"
        );
    }

    #[test]
    fn nested_soa_read_gen() {
        use check::*;
        let test_a = Struct {
            fields: vec![
                StructField {
                    name: "first",
                    r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                    array: true,
                    optional: false,
                },
                StructField {
                    name: "second",
                    r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                    array: true,
                    optional: false,
                },
            ],
        };
        let test_b = Export {
            name: "TestB",
            r#struct: Struct {
                fields: vec![StructField {
                    name: "test_a",
                    r#type: Ptr::new(("TestA", ResolvedType::Struct(test_a))),
                    array: true,
                    optional: false,
                }],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_read_impl("TestB", &test_b);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn read(reader: &mut packet::reader::Reader, output: &mut TestB) -> Result<(), packet::Error> {
    let output_test_a_len = reader.read_uint32()? as usize;
    output.test_a.reserve(output_test_a_len);
    for _ in 0..output_test_a_len {
        let mut output_test_a_item = TestA::default();
        let output_test_a_item_first_len = reader.read_uint32()? as usize;
        output_test_a_item.first.reserve(output_test_a_item_first_len);
        for _ in 0..output_test_a_item_first_len {
            let mut output_test_a_item_first_item = u8::default();
            output_test_a_item_first_item = reader.read_uint8()?;
            output_test_a_item.first.push(output_test_a_item_first_item);
        }
        let output_test_a_item_second_len = reader.read_uint32()? as usize;
        output_test_a_item.second.reserve(output_test_a_item_second_len);
        for _ in 0..output_test_a_item_second_len {
            let mut output_test_a_item_second_item = u8::default();
            output_test_a_item_second_item = reader.read_uint8()?;
            output_test_a_item.second.push(output_test_a_item_second_item);
        }
        output.test_a.push(output_test_a_item);
    }
    Ok(())
}
"
        );
    }

    #[test]
    fn complex_struct_write_gen() {
        use check::*;
        let flag = Enum {
            repr: EnumRepr::U8,
            variants: vec![EnumVariant { name: "A", value: 0 }, EnumVariant { name: "B", value: 1 }],
        };
        let position = Struct {
            fields: vec![
                StructField {
                    name: "x",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
                StructField {
                    name: "y",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
            ],
        };
        let test = Export {
            name: "Test",
            r#struct: Struct {
                fields: vec![
                    StructField {
                        name: "builtin_scalar",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "builtin_array",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "string_scalar",
                        r#type: Ptr::new(("string", ResolvedType::Builtin(Builtin::String))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "string_array",
                        r#type: Ptr::new(("string", ResolvedType::Builtin(Builtin::String))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "enum_scalar",
                        r#type: Ptr::new(("Flag", ResolvedType::Enum(flag.clone()))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "enum_array",
                        r#type: Ptr::new(("Flag", ResolvedType::Enum(flag.clone()))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "struct_scalar",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "struct_array",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "opt_scalar",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: true,
                    },
                    StructField {
                        name: "opt_enum",
                        r#type: Ptr::new(("Flag", ResolvedType::Enum(flag.clone()))),
                        array: false,
                        optional: true,
                    },
                    StructField {
                        name: "opt_struct",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                        array: false,
                        optional: true,
                    },
                ],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_write_impl("Test", &test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn write(writer: &mut packet::writer::Writer, input: &Test) {
    writer.write_uint8(input.builtin_scalar);
    writer.write_uint32(input.builtin_array.len() as u32);
    for input_builtin_array_item in input.builtin_array.iter() {
        writer.write_uint8(input_builtin_array_item);
    }
    writer.write_uint32(input.string_scalar.len() as u32);
    writer.write_string(&input.string_scalar);
    writer.write_uint32(input.string_array.len() as u32);
    for input_string_array_item in input.string_array.iter() {
        writer.write_uint32(input_string_array_item.len() as u32);
        writer.write_string(&input_string_array_item);
    }
    writer.write_uint8(input.enum_scalar as u8);
    writer.write_uint32(input.enum_array.len() as u32);
    for input_enum_array_item in input.enum_array.iter() {
        writer.write_uint8(input_enum_array_item as u8);
    }
    writer.write_float(input.struct_scalar.x);
    writer.write_float(input.struct_scalar.y);
    writer.write_uint32(input.struct_array.len() as u32);
    for input_struct_array_item in input.struct_array.iter() {
        writer.write_float(input_struct_array_item.x);
        writer.write_float(input_struct_array_item.y);
    }
    match input.opt_scalar {
        None => writer.write_uint8(0u8),
        Some(input_opt_scalar) => {
            writer.write_uint8(1u8);
            writer.write_uint8(input_opt_scalar);
        }
    }
    match input.opt_enum {
        None => writer.write_uint8(0u8),
        Some(input_opt_enum) => {
            writer.write_uint8(1u8);
            writer.write_uint8(input_opt_enum as u8);
        }
    }
    match &input.opt_struct {
        None => writer.write_uint8(0u8),
        Some(input_opt_struct) => {
            writer.write_uint8(1u8);
            writer.write_float(input_opt_struct.x);
            writer.write_float(input_opt_struct.y);
        }
    }
}
"
        );
    }

    #[test]
    fn complex_struct_read_gen() {
        use check::*;
        let flag = Enum {
            repr: EnumRepr::U8,
            variants: vec![EnumVariant { name: "A", value: 0 }, EnumVariant { name: "B", value: 1 }],
        };
        let position = Struct {
            fields: vec![
                StructField {
                    name: "x",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
                StructField {
                    name: "y",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
            ],
        };
        let test = Export {
            name: "Test",
            r#struct: Struct {
                fields: vec![
                    StructField {
                        name: "builtin_scalar",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "builtin_array",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "string_scalar",
                        r#type: Ptr::new(("string", ResolvedType::Builtin(Builtin::String))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "string_array",
                        r#type: Ptr::new(("string", ResolvedType::Builtin(Builtin::String))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "enum_scalar",
                        r#type: Ptr::new(("Flag", ResolvedType::Enum(flag.clone()))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "enum_array",
                        r#type: Ptr::new(("Flag", ResolvedType::Enum(flag.clone()))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "struct_scalar",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "struct_array",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                        array: true,
                        optional: false,
                    },
                    StructField {
                        name: "opt_scalar",
                        r#type: Ptr::new(("uint8", ResolvedType::Builtin(Builtin::Uint8))),
                        array: false,
                        optional: true,
                    },
                    StructField {
                        name: "opt_enum",
                        r#type: Ptr::new(("Flag", ResolvedType::Enum(flag.clone()))),
                        array: false,
                        optional: true,
                    },
                    StructField {
                        name: "opt_struct",
                        r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                        array: false,
                        optional: true,
                    },
                ],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_read_impl("Test", &test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn read(reader: &mut packet::reader::Reader, output: &mut Test) -> Result<(), packet::Error> {
    output.builtin_scalar = reader.read_uint8()?;
    let output_builtin_array_len = reader.read_uint32()? as usize;
    output.builtin_array.reserve(output_builtin_array_len);
    for _ in 0..output_builtin_array_len {
        let mut output_builtin_array_item = u8::default();
        output_builtin_array_item = reader.read_uint8()?;
        output.builtin_array.push(output_builtin_array_item);
    }
    let output_string_scalar_len = reader.read_uint32()? as usize;
    output.string_scalar = reader.read_string(output_string_scalar_len)?;
    let output_string_array_len = reader.read_uint32()? as usize;
    output.string_array.reserve(output_string_array_len);
    for _ in 0..output_string_array_len {
        let mut output_string_array_item = String::default();
        let output_string_array_item_len = reader.read_uint32()? as usize;
        output_string_array_item = reader.read_string(output_string_array_item_len)?;
        output.string_array.push(output_string_array_item);
    }
    output.enum_scalar = Flag::try_from(reader.read_uint8()?)?;
    let output_enum_array_len = reader.read_uint32()? as usize;
    output.enum_array.reserve(output_enum_array_len);
    for _ in 0..output_enum_array_len {
        let mut output_enum_array_item = Flag::default();
        output_enum_array_item = Flag::try_from(reader.read_uint8()?)?;
        output.enum_array.push(output_enum_array_item);
    }
    output.struct_scalar.x = reader.read_float()?;
    output.struct_scalar.y = reader.read_float()?;
    let output_struct_array_len = reader.read_uint32()? as usize;
    output.struct_array.reserve(output_struct_array_len);
    for _ in 0..output_struct_array_len {
        let mut output_struct_array_item = Position::default();
        output_struct_array_item.x = reader.read_float()?;
        output_struct_array_item.y = reader.read_float()?;
        output.struct_array.push(output_struct_array_item);
    }
    if reader.read_uint8()? > 0 {
        let mut output_opt_scalar = u8::default();
        output_opt_scalar = reader.read_uint8()?;
        output.opt_scalar = Some(output_opt_scalar);
    }
    if reader.read_uint8()? > 0 {
        let mut output_opt_enum = Flag::default();
        output_opt_enum = Flag::try_from(reader.read_uint8()?)?;
        output.opt_enum = Some(output_opt_enum);
    }
    if reader.read_uint8()? > 0 {
        let mut output_opt_struct = Position::default();
        output_opt_struct.x = reader.read_float()?;
        output_opt_struct.y = reader.read_float()?;
        output.opt_struct = Some(output_opt_struct);
    }
    Ok(())
}
"
        );
    }

    #[test]
    fn nested_write_opt_gen() {
        use check::*;
        let position = Struct {
            fields: vec![
                StructField {
                    name: "x",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
                StructField {
                    name: "y",
                    r#type: Ptr::new(("float", ResolvedType::Builtin(Builtin::Float))),
                    array: false,
                    optional: false,
                },
            ],
        };
        let entity = Struct {
            fields: vec![
                StructField {
                    name: "uid",
                    r#type: Ptr::new(("uint32", ResolvedType::Builtin(Builtin::Uint32))),
                    array: false,
                    optional: false,
                },
                StructField {
                    name: "pos",
                    r#type: Ptr::new(("Position", ResolvedType::Struct(position.clone()))),
                    array: false,
                    optional: true,
                },
            ],
        };
        let state = Export {
            name: "State",
            r#struct: Struct {
                fields: vec![
                    StructField {
                        name: "id",
                        r#type: Ptr::new(("uint32", ResolvedType::Builtin(Builtin::Uint32))),
                        array: false,
                        optional: false,
                    },
                    StructField {
                        name: "entities",
                        r#type: Ptr::new(("Entity", ResolvedType::Struct(entity.clone()))),
                        array: true,
                        optional: false,
                    },
                ],
            },
        };
        let mut gen = Generator::<Rust>::new();
        gen.push_line();
        gen.push_write_impl("State", &state);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
pub fn write(writer: &mut packet::writer::Writer, input: &State) {
    writer.write_uint32(input.id);
    writer.write_uint32(input.entities.len() as u32);
    for input_entities_item in input.entities.iter() {
        writer.write_uint32(input_entities_item.uid);
        match &input_entities_item.pos {
            None => writer.write_uint8(0u8),
            Some(input_entities_item_pos) => {
                writer.write_uint8(1u8);
                writer.write_float(input_entities_item_pos.x);
                writer.write_float(input_entities_item_pos.y);
            }
        }
    }
}
"
        );
    }
}
