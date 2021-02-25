use std::collections::HashSet;
use std::rc::Rc;

use fstrings::{format_args_f, format_f};

use super::*;

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TypeScript {
    imports: HashSet<String>,
}
impl Language for TypeScript {}

impl Common for TypeScript {
    fn gen_common(&self, out: &mut String) {
        append!(out, "import {{ Reader, Writer }} from \"packet\";\n");
    }
}

fn varname(stack: &[String], name: &str) -> String { format!("{}_{}", stack.join("_"), name) }
fn bindname(stack: &[String]) -> String { stack.join("_") }
fn fname(stack: &[String]) -> String { stack.join(".") }

fn gen_write_impl_optional(ctx: &mut GenCtx, body: impl Fn(&mut GenCtx)) {
    let fname = self::fname(&ctx.stack);
    let bind_var = bindname(&ctx.stack);
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(bind_var.clone());

    cat!(ctx, "let {bind_var} = {fname};\n");
    cat!(ctx, "switch ({bind_var}) {{\n");
    cat!(ctx +++);
    cat!(ctx, "case undefined: case null: writer.write_uint8(0); break;\n");
    cat!(ctx, "default: {{\n");
    cat!(ctx +++);
    cat!(ctx, "writer.write_uint8(1);\n");

    body(ctx);

    cat!(ctx ---);
    cat!(ctx, "}}\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_write_impl_array(ctx: &mut GenCtx, body: impl Fn(&mut GenCtx)) {
    let fname = self::fname(&ctx.stack);
    let item = varname(&ctx.stack, "item");
    let index = varname(&ctx.stack, "index");
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(item.clone());

    cat!(ctx, "writer.write_uint32({fname}.length);\n");
    cat!(ctx, "for (let {index} = 0; {index} < {fname}.length; ++{index}) {{\n");
    //cat!(ctx, "for (let {item} of {fname}) {{\n");
    cat!(ctx +++);
    cat!(ctx, "let {item} = {fname}[{index}];\n");

    body(ctx);

    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_write_impl_builtin(ctx: &mut GenCtx, ty: &check::Builtin, name: &str) {
    let fname = self::fname(&ctx.stack);
    match ty {
        check::Builtin::String => {
            cat!(ctx, "writer.write_uint32({fname}.length);\n");
            cat!(ctx, "writer.write_string({fname});\n");
        }
        _ => cat!(ctx, "writer.write_{name}({fname});\n"),
    }
}

fn gen_write_impl_enum(ctx: &mut GenCtx, type_info: &check::Enum, _name: &str) {
    let fname = self::fname(&ctx.stack);
    let repr_name = match &type_info.repr {
        check::EnumRepr::U8 => "uint8",
        check::EnumRepr::U16 => "uint16",
        check::EnumRepr::U32 => "uint32",
    };

    cat!(ctx, "writer.write_{repr_name}({fname} as number);\n");
}

fn gen_write_impl_struct(ctx: &mut GenCtx, ty: &check::Struct, _name: &str) {
    for f in &ty.fields {
        ctx.push_fname(f.name);
        let fty = &*f.r#type.borrow();

        use check::ResolvedType::*;
        let mut generator: Box<dyn Fn(&mut GenCtx)> = match &fty.1 {
            Builtin(fty_info) => Box::new(move |ctx| gen_write_impl_builtin(ctx, &fty_info, &fty.0)),
            Enum(fty_info) => Box::new(move |ctx| gen_write_impl_enum(ctx, &fty_info, &fty.0)),
            Struct(fty_info) => Box::new(move |ctx| gen_write_impl_struct(ctx, &fty_info, &fty.0)),
        };
        if f.array {
            generator = Box::new(move |ctx| gen_write_impl_array(ctx, |ctx| generator(ctx)))
        }
        if f.optional {
            generator = Box::new(move |ctx| gen_write_impl_optional(ctx, |ctx| generator(ctx)))
        }
        generator(ctx);

        ctx.pop_fname();
    }
}

fn gen_read_impl_optional(ctx: &mut GenCtx, init_item: bool, body: impl Fn(&mut GenCtx)) {
    let fname = self::fname(&ctx.stack);
    let bind_var = bindname(&ctx.stack);
    let old_stack = if init_item {
        let mut old_stack = Vec::new();
        ctx.swap_stack(&mut old_stack);
        ctx.push_fname(bind_var.clone());

        Some(old_stack)
    } else {
        None
    };

    cat!(ctx, "if (reader.read_uint8() > 0) {{\n");
    cat!(ctx +++);
    if init_item {
        cat!(ctx, "let {bind_var}: any = {{}};\n")
    }

    body(ctx);

    if init_item {
        cat!(ctx, "{fname} = {bind_var};\n");
    }
    cat!(ctx ---);
    cat!(ctx, "}} else {{\n");
    cat!(ctx +++);
    cat!(ctx, "{fname} = undefined;\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");

    if let Some(mut old_stack) = old_stack {
        ctx.swap_stack(&mut old_stack);
    }
}

fn gen_read_impl_array(ctx: &mut GenCtx, init_item: bool, body: impl Fn(&mut GenCtx)) {
    let len_var = varname(&ctx.stack, "len");
    let fname = self::fname(&ctx.stack);
    let idx_var = varname(&ctx.stack, "index");
    let item_var = varname(&ctx.stack, "item");
    let mut old_stack = Vec::new();
    ctx.swap_stack(&mut old_stack);
    ctx.push_fname(item_var.clone());

    cat!(ctx, "let {len_var} = reader.read_uint32();\n");
    cat!(ctx, "{fname} = new Array({len_var});\n");
    cat!(ctx, "for (let {idx_var} = 0; {idx_var} < {len_var}; ++{idx_var}) {{\n");
    cat!(ctx +++);
    if init_item {
        cat!(ctx, "let {item_var}: any = {{}};\n");
    } else {
        cat!(ctx, "let {item_var};\n");
    }

    body(ctx);

    cat!(ctx, "{fname}[{idx_var}] = {item_var};\n");
    cat!(ctx ---);
    cat!(ctx, "}}\n");

    ctx.swap_stack(&mut old_stack);
}

fn gen_read_impl_builtin(ctx: &mut GenCtx, type_info: &check::Builtin, type_name: &str) {
    match type_info {
        check::Builtin::String => {
            let len_var = varname(&ctx.stack, "len");
            cat!(ctx, "let {len_var} = reader.read_uint32();\n");
            let fname = self::fname(&ctx.stack);
            cat!(ctx, "{fname} = reader.read_string({len_var});\n");
        }
        _ => {
            let fname = self::fname(&ctx.stack);
            cat!(ctx, "{fname} = reader.read_{type_name}();\n")
        }
    }
}

fn gen_read_impl_enum(ctx: &mut GenCtx, type_info: &check::Enum, _name: &str) {
    let repr_name = match type_info.repr {
        check::EnumRepr::U8 => "uint8",
        check::EnumRepr::U16 => "uint16",
        check::EnumRepr::U32 => "uint32",
    };
    let (min, max) = (
        1 << type_info.variants[0].value,
        1 << type_info.variants[type_info.variants.len() - 1].value,
    );
    let fname = self::fname(&ctx.stack);
    let temp = self::varname(&ctx.stack, "temp");
    cat!(ctx, "let {temp} = reader.read_{repr_name}();\n");
    cat!(ctx, "if ({min} <= {temp} && {temp} <= {max}) {fname} = {temp};\n");
    cat!(ctx, "else reader.failed = true;\n");
}

fn gen_read_impl_struct(ctx: &mut GenCtx, ty: &check::Struct, _name: &str) {
    for f in &ty.fields {
        ctx.push_fname(f.name);
        let fty = &*f.r#type.borrow();

        use check::ResolvedType::*;
        let mut init_item = false;
        let mut generator: Rc<dyn Fn(&mut GenCtx)> = match &fty.1 {
            Builtin(fty_info) => Rc::new(move |ctx| gen_read_impl_builtin(ctx, &fty_info, &fty.0)),
            Enum(fty_info) => Rc::new(move |ctx| gen_read_impl_enum(ctx, &fty_info, &fty.0)),
            Struct(fty_info) => {
                init_item = true;
                Rc::new(move |ctx| gen_read_impl_struct(ctx, &fty_info, &fty.0))
            }
        };
        if f.array {
            let current_generator = generator.clone();
            generator = Rc::new(move |ctx| gen_read_impl_array(ctx, init_item, |ctx| current_generator(ctx)))
        }
        if f.optional {
            let current_generator = generator.clone();
            generator = Rc::new(move |ctx| gen_read_impl_optional(ctx, init_item, |ctx| current_generator(ctx)))
        }
        generator(ctx);
        ctx.pop_fname();
    }
}

fn field_ctor_type(ty: &(&str, check::ResolvedType), export: &str, array: bool, optional: bool) -> String {
    let (mut name, rty) = ty;
    let mut needs_export_prefix = false;
    match *rty {
        check::ResolvedType::Builtin(check::Builtin::String) => name = "string",
        check::ResolvedType::Builtin(_) => name = "number",
        _ => needs_export_prefix = true,
    }
    format_f!(
        "{prefix}{dot}{name}{arr}{opt}",
        prefix = if needs_export_prefix { export } else { "" },
        dot = if needs_export_prefix { "." } else { "" },
        arr = if array { "[]" } else { "" },
        opt = if optional { " | undefined" } else { "" }
    )
}

impl Impl for TypeScript {
    fn gen_impl<'a>(&self, export: &check::Export, out: &mut String) {
        let mut ctx = GenCtx::new(out);

        cat!(ctx, "export class {export.name} {{\n");
        cat!(ctx +++);
        cat!(ctx, "constructor(\n");
        cat!(ctx +++);
        for field in export.r#struct.fields.iter() {
            let field_type = field_ctor_type(&(*field.r#type.borrow()), export.name, field.array, field.optional);
            cat!(ctx, "public {field.name}: {field_type},\n");
        }
        cat!(ctx ---);
        cat!(ctx, ") {{}}\n");

        ctx.push_fname("output");
        cat!(ctx, "static read(data: ArrayBuffer): {export.name} | null {{\n");
        cat!(ctx +++);
        cat!(ctx, "let reader = new Reader(data);\n");
        cat!(ctx, "let output = Object.create({export.name});\n");
        gen_read_impl_struct(&mut ctx, &export.r#struct, &export.name);
        cat!(ctx, "if (reader.failed) return null;\n");
        cat!(ctx, "return output;\n");
        cat!(ctx ---);
        cat!(ctx, "}}\n");
        ctx.pop_fname();

        ctx.push_fname("this");
        cat!(ctx, "write(buffer?: ArrayBuffer): ArrayBuffer {{\n");
        cat!(ctx +++);
        cat!(ctx, "let writer = buffer ? new Writer(buffer) : new Writer();\n");
        gen_write_impl_struct(&mut ctx, &export.r#struct, &export.name);
        cat!(ctx, "return writer.finish();\n");
        cat!(ctx ---);
        cat!(ctx, "}}\n");
        ctx.pop_fname();

        cat!(ctx ---);
        cat!(ctx, "}}\n");
    }
}

fn gen_struct_decl(ctx: &mut GenCtx, ty: &check::Struct, name: &str) {
    cat!(ctx, "export interface {name} {{\n");
    cat!(ctx +++);
    for field in ty.fields.iter() {
        let type_info = &*field.r#type.borrow();
        let typename: &str = match &type_info.1 {
            check::ResolvedType::Builtin(b) => match b {
                check::Builtin::String => "string",
                _ => "number",
            },
            _ => &type_info.0,
        };
        let opt = if field.optional { "?" } else { "" };
        let arr = if field.array { "[]" } else { "" };

        cat!(ctx, "{field.name}{opt}: {typename}{arr},\n");
    }
    cat!(ctx ---);
    cat!(ctx, "}}\n");
}

fn gen_enum_decl(ctx: &mut GenCtx, ty: &check::Enum, name: &str) {
    cat!(ctx, "export const enum {name} {{\n");
    cat!(ctx +++);
    for variant in ty.variants.iter() {
        cat!(ctx, "{variant.name} = 1 << {variant.value},\n");
    }
    cat!(ctx ---);
    cat!(ctx, "}}\n");
}

impl Declaration for TypeScript {
    fn gen_decls<'a>(&self, types: &check::TypeMap<'a>, export: &str, out: &mut String) {
        let mut ctx = GenCtx::new(out);

        cat!(ctx, "export namespace {export} {{\n");
        cat!(ctx +++);
        for (name, ty) in types.iter() {
            if *name == export {
                continue;
            }

            match &(*ty.borrow()).1 {
                check::ResolvedType::Builtin(_) => (),
                check::ResolvedType::Enum(ty) => gen_enum_decl(&mut ctx, ty, name),
                check::ResolvedType::Struct(ty) => gen_struct_decl(&mut ctx, ty, name),
            }
        }
        cat!(ctx ---);
        cat!(ctx, "}}\n");
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn commmon_gen() {
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_common();
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
import { Reader, Writer } from \"packet\";
"
        );
    }

    #[test]
    fn simple_struct_gen() {
        use check::*;
        let mut types = TypeMap::new();
        types.insert(
            "Position",
            Ptr::new((
                "Position",
                ResolvedType::Struct(Struct {
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
                }),
            )),
        );
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_decls(&types, "Test");
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export namespace Test {
    export interface Position {
        x: number,
        y: number,
    }
}
"
        );
    }

    #[test]
    fn struct_with_optional_gen() {
        use check::*;
        let mut types = TypeMap::new();
        types.insert(
            "A",
            Ptr::new((
                "A",
                ResolvedType::Struct(Struct {
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
                }),
            )),
        );
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_decls(&types, "Test");
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export namespace Test {
    export interface A {
        a?: number,
        b?: number[],
        c: number,
    }
}
"
        );
    }

    #[test]
    fn enum_gen() {
        use check::*;
        let mut types = TypeMap::new();
        types.insert(
            "Flag",
            Ptr::new((
                "Flag",
                ResolvedType::Enum(Enum {
                    repr: EnumRepr::U8,
                    variants: vec![EnumVariant { name: "A", value: 0 }, EnumVariant { name: "B", value: 1 }],
                }),
            )),
        );
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_decls(&types, "Test");
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export namespace Test {
    export const enum Flag {
        A = 1 << 0,
        B = 1 << 1,
    }
}
"
        );
    }

    #[test]
    fn complex_struct_gen() {
        use check::*;
        let mut types = TypeMap::new();
        types.insert(
            "A",
            Ptr::new((
                "A",
                ResolvedType::Struct(Struct {
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
                }),
            )),
        );
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_decls(&types, "Test");
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export namespace Test {
    export interface A {
        builtin_scalar: number,
        builtin_array: number[],
        string_scalar: string,
        string_array: string[],
        enum_scalar: Flag,
        enum_array: Flag[],
        struct_scalar: Position,
        struct_array: Position[],
    }
}
"
        );
    }

    #[test]
    fn optional_impl_gen() {
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
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_impl(&test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export class Test {
    constructor(
        public a: number | undefined,
        public b: number[] | undefined,
        public c: number,
    ) {}
    static read(data: ArrayBuffer): Test | null {
        let reader = new Reader(data);
        let output = Object.create(Test);
        if (reader.read_uint8() > 0) {
            output.a = reader.read_uint8();
        } else {
            output.a = undefined;
        }
        if (reader.read_uint8() > 0) {
            let output_b_len = reader.read_uint32();
            output.b = new Array(output_b_len);
            for (let output_b_index = 0; output_b_index < output_b_len; ++output_b_index) {
                let output_b_item;
                output_b_item = reader.read_uint8();
                output.b[output_b_index] = output_b_item;
            }
        } else {
            output.b = undefined;
        }
        output.c = reader.read_uint8();
        if (reader.failed) return null;
        return output;
    }
    write(buffer?: ArrayBuffer): ArrayBuffer {
        let writer = buffer ? new Writer(buffer) : new Writer();
        let this_a = this.a;
        switch (this_a) {
            case undefined: case null: writer.write_uint8(0); break;
            default: {
                writer.write_uint8(1);
                writer.write_uint8(this_a);
            }
        }
        let this_b = this.b;
        switch (this_b) {
            case undefined: case null: writer.write_uint8(0); break;
            default: {
                writer.write_uint8(1);
                writer.write_uint32(this_b.length);
                for (let this_b_index = 0; this_b_index < this_b.length; ++this_b_index) {
                    let this_b_item = this_b[this_b_index];
                    writer.write_uint8(this_b_item);
                }
            }
        }
        writer.write_uint8(this.c);
        return writer.finish();
    }
}
"
        );
    }

    #[test]
    fn nested_soa_impl_gen() {
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
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_impl(&test_b);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export class TestB {
    constructor(
        public test_a: TestB.TestA[],
    ) {}
    static read(data: ArrayBuffer): TestB | null {
        let reader = new Reader(data);
        let output = Object.create(TestB);
        let output_test_a_len = reader.read_uint32();
        output.test_a = new Array(output_test_a_len);
        for (let output_test_a_index = 0; output_test_a_index < output_test_a_len; ++output_test_a_index) {
            let output_test_a_item: any = {};
            let output_test_a_item_first_len = reader.read_uint32();
            output_test_a_item.first = new Array(output_test_a_item_first_len);
            for (let output_test_a_item_first_index = 0; output_test_a_item_first_index < output_test_a_item_first_len; ++output_test_a_item_first_index) {
                let output_test_a_item_first_item;
                output_test_a_item_first_item = reader.read_uint8();
                output_test_a_item.first[output_test_a_item_first_index] = output_test_a_item_first_item;
            }
            let output_test_a_item_second_len = reader.read_uint32();
            output_test_a_item.second = new Array(output_test_a_item_second_len);
            for (let output_test_a_item_second_index = 0; output_test_a_item_second_index < output_test_a_item_second_len; ++output_test_a_item_second_index) {
                let output_test_a_item_second_item;
                output_test_a_item_second_item = reader.read_uint8();
                output_test_a_item.second[output_test_a_item_second_index] = output_test_a_item_second_item;
            }
            output.test_a[output_test_a_index] = output_test_a_item;
        }
        if (reader.failed) return null;
        return output;
    }
    write(buffer?: ArrayBuffer): ArrayBuffer {
        let writer = buffer ? new Writer(buffer) : new Writer();
        writer.write_uint32(this.test_a.length);
        for (let this_test_a_index = 0; this_test_a_index < this.test_a.length; ++this_test_a_index) {
            let this_test_a_item = this.test_a[this_test_a_index];
            writer.write_uint32(this_test_a_item.first.length);
            for (let this_test_a_item_first_index = 0; this_test_a_item_first_index < this_test_a_item.first.length; ++this_test_a_item_first_index) {
                let this_test_a_item_first_item = this_test_a_item.first[this_test_a_item_first_index];
                writer.write_uint8(this_test_a_item_first_item);
            }
            writer.write_uint32(this_test_a_item.second.length);
            for (let this_test_a_item_second_index = 0; this_test_a_item_second_index < this_test_a_item.second.length; ++this_test_a_item_second_index) {
                let this_test_a_item_second_item = this_test_a_item.second[this_test_a_item_second_index];
                writer.write_uint8(this_test_a_item_second_item);
            }
        }
        return writer.finish();
    }
}
"
        );
    }

    #[test]
    fn complex_struct_impl_gen() {
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
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_impl(&test);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export class Test {
    constructor(
        public builtin_scalar: number,
        public builtin_array: number[],
        public string_scalar: string,
        public string_array: string[],
        public enum_scalar: Test.Flag,
        public enum_array: Test.Flag[],
        public struct_scalar: Test.Position,
        public struct_array: Test.Position[],
        public opt_scalar: number | undefined,
        public opt_enum: Test.Flag | undefined,
        public opt_struct: Test.Position | undefined,
    ) {}
    static read(data: ArrayBuffer): Test | null {
        let reader = new Reader(data);
        let output = Object.create(Test);
        output.builtin_scalar = reader.read_uint8();
        let output_builtin_array_len = reader.read_uint32();
        output.builtin_array = new Array(output_builtin_array_len);
        for (let output_builtin_array_index = 0; output_builtin_array_index < output_builtin_array_len; ++output_builtin_array_index) {
            let output_builtin_array_item;
            output_builtin_array_item = reader.read_uint8();
            output.builtin_array[output_builtin_array_index] = output_builtin_array_item;
        }
        let output_string_scalar_len = reader.read_uint32();
        output.string_scalar = reader.read_string(output_string_scalar_len);
        let output_string_array_len = reader.read_uint32();
        output.string_array = new Array(output_string_array_len);
        for (let output_string_array_index = 0; output_string_array_index < output_string_array_len; ++output_string_array_index) {
            let output_string_array_item;
            let output_string_array_item_len = reader.read_uint32();
            output_string_array_item = reader.read_string(output_string_array_item_len);
            output.string_array[output_string_array_index] = output_string_array_item;
        }
        let output_enum_scalar_temp = reader.read_uint8();
        if (1 <= output_enum_scalar_temp && output_enum_scalar_temp <= 2) output.enum_scalar = output_enum_scalar_temp;
        else reader.failed = true;
        let output_enum_array_len = reader.read_uint32();
        output.enum_array = new Array(output_enum_array_len);
        for (let output_enum_array_index = 0; output_enum_array_index < output_enum_array_len; ++output_enum_array_index) {
            let output_enum_array_item;
            let output_enum_array_item_temp = reader.read_uint8();
            if (1 <= output_enum_array_item_temp && output_enum_array_item_temp <= 2) output_enum_array_item = output_enum_array_item_temp;
            else reader.failed = true;
            output.enum_array[output_enum_array_index] = output_enum_array_item;
        }
        output.struct_scalar.x = reader.read_float();
        output.struct_scalar.y = reader.read_float();
        let output_struct_array_len = reader.read_uint32();
        output.struct_array = new Array(output_struct_array_len);
        for (let output_struct_array_index = 0; output_struct_array_index < output_struct_array_len; ++output_struct_array_index) {
            let output_struct_array_item: any = {};
            output_struct_array_item.x = reader.read_float();
            output_struct_array_item.y = reader.read_float();
            output.struct_array[output_struct_array_index] = output_struct_array_item;
        }
        if (reader.read_uint8() > 0) {
            output.opt_scalar = reader.read_uint8();
        } else {
            output.opt_scalar = undefined;
        }
        if (reader.read_uint8() > 0) {
            let output_opt_enum_temp = reader.read_uint8();
            if (1 <= output_opt_enum_temp && output_opt_enum_temp <= 2) output.opt_enum = output_opt_enum_temp;
            else reader.failed = true;
        } else {
            output.opt_enum = undefined;
        }
        if (reader.read_uint8() > 0) {
            let output_opt_struct: any = {};
            output_opt_struct.x = reader.read_float();
            output_opt_struct.y = reader.read_float();
            output.opt_struct = output_opt_struct;
        } else {
            output.opt_struct = undefined;
        }
        if (reader.failed) return null;
        return output;
    }
    write(buffer?: ArrayBuffer): ArrayBuffer {
        let writer = buffer ? new Writer(buffer) : new Writer();
        writer.write_uint8(this.builtin_scalar);
        writer.write_uint32(this.builtin_array.length);
        for (let this_builtin_array_index = 0; this_builtin_array_index < this.builtin_array.length; ++this_builtin_array_index) {
            let this_builtin_array_item = this.builtin_array[this_builtin_array_index];
            writer.write_uint8(this_builtin_array_item);
        }
        writer.write_uint32(this.string_scalar.length);
        writer.write_string(this.string_scalar);
        writer.write_uint32(this.string_array.length);
        for (let this_string_array_index = 0; this_string_array_index < this.string_array.length; ++this_string_array_index) {
            let this_string_array_item = this.string_array[this_string_array_index];
            writer.write_uint32(this_string_array_item.length);
            writer.write_string(this_string_array_item);
        }
        writer.write_uint8(this.enum_scalar as number);
        writer.write_uint32(this.enum_array.length);
        for (let this_enum_array_index = 0; this_enum_array_index < this.enum_array.length; ++this_enum_array_index) {
            let this_enum_array_item = this.enum_array[this_enum_array_index];
            writer.write_uint8(this_enum_array_item as number);
        }
        writer.write_float(this.struct_scalar.x);
        writer.write_float(this.struct_scalar.y);
        writer.write_uint32(this.struct_array.length);
        for (let this_struct_array_index = 0; this_struct_array_index < this.struct_array.length; ++this_struct_array_index) {
            let this_struct_array_item = this.struct_array[this_struct_array_index];
            writer.write_float(this_struct_array_item.x);
            writer.write_float(this_struct_array_item.y);
        }
        let this_opt_scalar = this.opt_scalar;
        switch (this_opt_scalar) {
            case undefined: case null: writer.write_uint8(0); break;
            default: {
                writer.write_uint8(1);
                writer.write_uint8(this_opt_scalar);
            }
        }
        let this_opt_enum = this.opt_enum;
        switch (this_opt_enum) {
            case undefined: case null: writer.write_uint8(0); break;
            default: {
                writer.write_uint8(1);
                writer.write_uint8(this_opt_enum as number);
            }
        }
        let this_opt_struct = this.opt_struct;
        switch (this_opt_struct) {
            case undefined: case null: writer.write_uint8(0); break;
            default: {
                writer.write_uint8(1);
                writer.write_float(this_opt_struct.x);
                writer.write_float(this_opt_struct.y);
            }
        }
        return writer.finish();
    }
}
"
        );
    }

    #[test]
    fn nested_struct_with_opt_impl_gen() {
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
        let mut gen = Generator::<TypeScript>::new();
        gen.push_line();
        gen.push_impl(&state);
        let actual = gen.finish();
        assert_eq!(
            actual,
            "
export class State {
    constructor(
        public id: number,
        public entities: State.Entity[],
    ) {}
    static read(data: ArrayBuffer): State | null {
        let reader = new Reader(data);
        let output = Object.create(State);
        output.id = reader.read_uint32();
        let output_entities_len = reader.read_uint32();
        output.entities = new Array(output_entities_len);
        for (let output_entities_index = 0; output_entities_index < output_entities_len; ++output_entities_index) {
            let output_entities_item: any = {};
            output_entities_item.uid = reader.read_uint32();
            if (reader.read_uint8() > 0) {
                let output_entities_item_pos: any = {};
                output_entities_item_pos.x = reader.read_float();
                output_entities_item_pos.y = reader.read_float();
                output_entities_item.pos = output_entities_item_pos;
            } else {
                output_entities_item.pos = undefined;
            }
            output.entities[output_entities_index] = output_entities_item;
        }
        if (reader.failed) return null;
        return output;
    }
    write(buffer?: ArrayBuffer): ArrayBuffer {
        let writer = buffer ? new Writer(buffer) : new Writer();
        writer.write_uint32(this.id);
        writer.write_uint32(this.entities.length);
        for (let this_entities_index = 0; this_entities_index < this.entities.length; ++this_entities_index) {
            let this_entities_item = this.entities[this_entities_index];
            writer.write_uint32(this_entities_item.uid);
            let this_entities_item_pos = this_entities_item.pos;
            switch (this_entities_item_pos) {
                case undefined: case null: writer.write_uint8(0); break;
                default: {
                    writer.write_uint8(1);
                    writer.write_float(this_entities_item_pos.x);
                    writer.write_float(this_entities_item_pos.y);
                }
            }
        }
        return writer.finish();
    }
}
"
        );
    }
}
