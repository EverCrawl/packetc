#![feature(test)]
extern crate packetc_lib;
extern crate test;

use packetc_lib::{
    check,
    gen::{self, /* rust::Rust, */ ts::TypeScript},
    parser,
};
use test::Bencher;

const BENCH_INPUT_3KB: &str = "resource/bench3KB.pkt";

fn load_bench(name: &str) -> String { std::fs::read_to_string(name).expect("Unknown bench path") }

/* #[bench]
fn codegen_rust_3kb(b: &mut Bencher) {
    let bench = load_bench(BENCH_INPUT_3KB);
    let ast = parser::pkt::schema(&bench).unwrap();
    let resolved = check::type_check(ast.clone()).unwrap();
    b.iter(move || gen::generate::<Rust>(&resolved));
} */

#[bench]
fn codegen_typescript_3kb(b: &mut Bencher) {
    let bench = load_bench(BENCH_INPUT_3KB);
    let ast = parser::pkt::schema(&bench).unwrap();
    let resolved = check::type_check(ast.clone()).unwrap();
    b.iter(move || gen::generate::<TypeScript>(&resolved));
}
