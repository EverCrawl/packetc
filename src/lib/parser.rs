use ast::*;

use super::ast;

// TODO: allow specifying max array size -> use it to shrink array len encoding
// if possible right now, it's uint32 by default, which is very wasteful

peg::parser!(pub grammar pkt() for str {
    /// Parses whitespace
    rule _() = [' ' | '\t']*
    /// Parses newlines
    rule __() = ['\n' | '\r']*
    /// Parses whitespace or newlines
    rule ___() = [' ' | '\t' | '\n' | '\r']*

    /// Parses a single-line comment
    rule comment()
        = "#" [ch if ch != '\n']* __

    rule string() -> &'input str
        = s:$(['a'..='z'|'A'..='Z'|'0'..='9'|'_']*) { s }

    /// Parses reserved keywords (the base types + enum/struct keywords)
    rule reserved()
        = "uint8"
        / "uint16"
        / "uint32"
        / "int8"
        / "int16"
        / "int32"
        / "float"
        / "string"
        / "enum"
        / "struct"
    /// Parses the first character of an identifier, which cannot contain numbers
    rule ident_start() -> &'input str = s:$(['a'..='z'|'A'..='Z'|'_']) { s }
    /// Parses any alphanumeric characters as part of an identifier
    rule ident_chars() -> &'input str = s:$(['a'..='z'|'A'..='Z'|'0'..='9'|'_']) { s }
    /// Parses an entire identifier, ensuring no reserved keywords are used
    rule ident() -> &'input str
        = i:quiet!{ $(!reserved() ident_start() ident_chars()*) } { i }

    rule enum_variant() -> &'input str
        = s:ident() ___ ","? ___ { s }
    /// Parses an enum in the form `identifier: enum { VARIANT_A, ... }`
    rule enum_type() -> Enum<'input>
        = _ "enum" _ "{" ___ variants:(enum_variant()*) ___ "}" { Enum(variants) }

    rule is_optional() -> bool
        = o:("?"?) { o.is_some() }

    rule struct_field() -> Option<(&'input str, Unresolved<'input>)>
        = comment() ___ { None }
        / i:ident() _ opt:is_optional() ":" _ t:string() a:("[]"?) ___ ","? ___ { Some((i, Unresolved(t, a.is_some(), opt))) }

    /// Parses a struct in the from `identifier: struct { name: type or type[], ... }
    rule struct_type() -> Struct<'input>
        = _ "struct" _ "{" ___ fields:(struct_field()*) ___ "}" {
            Struct(fields.into_iter()
            .filter_map(|x| x)
            .collect())
        }

    /// Recursively parses a type
    rule r#type() -> Type<'input>
        = e:enum_type() { Type::Enum(e) }
        / s:struct_type() { Type::Struct(s) }

    /// Parses a declaration in the form `identifier : type`
    rule decl() -> Node<'input>
        = _ i:ident() _ ":" _ t:r#type() ___ {
            Node::Decl(i, t)
        }

    rule export() -> Node<'input>
        = "export" _ s:string() {
            Node::Export(s)
        }

    rule line() -> Option<Node<'input>>
        = _ comment() __ { None }
        / _ e:(export()) __ { Some(e) }
        / _ s:(decl()) __ { Some(s) }

    /// Parses a schema file
    pub rule schema() -> AST<'input>
        = __? lines:(line()*) {
            lines.into_iter()
                .filter_map(|x| x)
                .collect()
        }
});

#[cfg(test)]
mod tests {
    use peg::str::LineCol;
    use pretty_assertions::assert_eq;

    use super::*;

    // This exists so that test case strings are leading whitespace insensitive
    trait TestCaseString {
        fn build(&self) -> String;
    }
    impl TestCaseString for str {
        fn build(&self) -> String {
            self.split('\n')
                .map(|s| s.trim_start())
                .collect::<Vec<&str>>()
                .join("\n")
        }
    }

    #[test]
    fn parse_whitespace_before_array_bracket() {
        let test = r#"
        a: struct { v: uint8 [] }
        "#
        .build();
        let expected = LineCol {
            line: 2,
            column: 22,
            offset: 22,
        };
        let actual = pkt::schema(&test).unwrap_err().location;
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_unclosed_struct_brackets() {
        let test = r#"
        aaa: struct { x:float, y:float
        "#
        .build();
        let expected = LineCol {
            line: 3,
            column: 1,
            offset: 32,
        };
        let actual = pkt::schema(&test).unwrap_err().location;
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_unclosed_enum_brackets() {
        let test = r#"
        aaa: enum { A, B
        "#
        .build();
        let expected = LineCol {
            line: 3,
            column: 1,
            offset: 18,
        };
        let actual = pkt::schema(&test).unwrap_err().location;
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_unclosed_array_brackets() {
        let test = r#"
        a: struct { v: uint8[ }
        "#
        .build();
        let expected = LineCol {
            line: 2,
            column: 21,
            offset: 21,
        };
        let actual = pkt::schema(&test).unwrap_err().location;
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_reserved_identifier() {
        let test = r#"
        uint8: uint8
        "#
        .build();
        let expected = LineCol {
            line: 2,
            column: 1,
            offset: 1,
        };
        let actual = pkt::schema(&test).unwrap_err().location;
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_first_char_numeric_bad_identifier() {
        let test = r#"
        0aaa: uint8
        "#
        .build();
        let expected = LineCol {
            line: 2,
            column: 1,
            offset: 1,
        };
        let actual = pkt::schema(&test).unwrap_err().location;
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_comment() {
        let test = r#"
        # this is a comment.
        "#
        .build();
        let expected: AST = vec![];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_comment_right_of_line() {
        let test = r#"
        a: struct { v: uint8 } # this is a comment placed to the right of a line.
        "#
        .build();
        let expected: AST = vec![Node::Decl(
            "a",
            Type::Struct(Struct(vec![("v", Unresolved("uint8", false, false))])),
        )];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_comment_inside_brackets() {
        let test = r#"
        a: struct {
            # this is a comment placed infront of fields
            a: uint8,
            # this is a comment placed inbetween fields
            b: uint8
            # this is a comment placed after of fields
        }
        "#
        .build();
        let expected: AST = vec![Node::Decl(
            "a",
            Type::Struct(Struct(vec![
                ("a", Unresolved("uint8", false, false)),
                ("b", Unresolved("uint8", false, false)),
            ])),
        )];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_enum() {
        let test = r#"
        asdf: enum { A, B }
        "#
        .build();
        let expected: AST = vec![Node::Decl("asdf", Type::Enum(Enum(vec!["A", "B"])))];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_struct() {
        let test = r#"
        asdf: struct {
            x: float,
            y: float
        }"#
        .build();
        let expected: AST = vec![Node::Decl(
            "asdf",
            Type::Struct(Struct(vec![
                ("x", Unresolved("float", false, false)),
                ("y", Unresolved("float", false, false)),
            ])),
        )];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_struct_trailing_comma() {
        let test = r#"
        asdf: struct {
            a: A,
            b: B,
        }
        "#
        .build();
        let expected: AST = vec![Node::Decl(
            "asdf",
            Type::Struct(Struct(vec![
                ("a", Unresolved("A", false, false)),
                ("b", Unresolved("B", false, false)),
            ])),
        )];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_struct_with_arrays() {
        let test = r#"
        asdf: struct {
            a: A[],
            b: B[],
        }
        "#
        .build();
        let expected: AST = vec![Node::Decl(
            "asdf",
            Type::Struct(Struct(vec![
                ("a", Unresolved("A", true, false)),
                ("b", Unresolved("B", true, false)),
            ])),
        )];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_struct_with_optional() {
        let test = r#"
        asdf: struct {
            a?: A[],
            b?: B,
            c: C
        }
        "#
        .build();
        let expected: AST = vec![Node::Decl(
            "asdf",
            Type::Struct(Struct(vec![
                ("a", Unresolved("A", true, true)),
                ("b", Unresolved("B", false, true)),
                ("c", Unresolved("C", false, false)),
            ])),
        )];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_export() {
        let test = r#"
        export Test
        "#
        .build();
        let expected: AST = vec![Node::Export("Test")];
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }

    #[test]
    fn parse_complex() {
        let test = r#"
        # This is a comment.
        # Below is what a fairly complex packet may look like
        Flag: enum { A, B }
        Position: struct { x: float, y: float }
        Value: struct { 
            a: uint32, b: int32, c: uint8, d: uint8
        }
        ComplexType: struct {
            flag: Flag,
            pos: Position,
            names: string[],
            values: Value[]
        }
        export ComplexType
        "#
        .build();
        let expected: AST = vec![
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
        assert_eq!(pkt::schema(&test).unwrap(), expected);
    }
}
