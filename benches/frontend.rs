#![feature(test)]
extern crate packetc_lib;
extern crate test;

use packetc_lib::*;
use test::Bencher;

const BENCH_INPUT_3KB: &str = "resource/bench3KB.pkt";
const BENCH_INPUT_64KB: &str = "resource/bench64KB.pkt";
const BENCH_INPUT_512KB: &str = "resource/bench512KB.pkt";

fn load_bench(name: &str) -> String { std::fs::read_to_string(name).expect("Unknown bench path") }

#[bench]
#[ignore]
fn ast_clone_baseline_64kb(b: &mut Bencher) {
    let bench = load_bench(BENCH_INPUT_64KB);
    let ast = parser::pkt::schema(&bench).unwrap();
    b.iter(move || ast.clone());
}

#[bench]
fn type_checker_3kb(b: &mut Bencher) {
    let bench = load_bench(BENCH_INPUT_3KB);
    let ast = parser::pkt::schema(&bench).unwrap();
    b.iter(move || check::type_check(ast.clone()));
}

#[bench]
fn type_checker_64kb(b: &mut Bencher) {
    let bench = load_bench(BENCH_INPUT_64KB);
    let ast = parser::pkt::schema(&bench).unwrap();
    b.iter(move || check::type_check(ast.clone()));
}

#[bench]
fn type_checker_512kb(b: &mut Bencher) {
    let bench = load_bench(BENCH_INPUT_512KB);
    let ast = parser::pkt::schema(&bench).unwrap();
    b.iter(move || check::type_check(ast.clone()));
}
