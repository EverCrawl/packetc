#![recursion_limit = "256"]
extern crate clap;
extern crate fstrings;
extern crate packetc_lib as pkt;

use std::path::PathBuf;
use std::{fs, path::Component, path::Path};

use anyhow::Result;
use clap::Clap;

#[derive(Clap)]
#[clap(version = "1.0", author = "Jan P. <honza.spacir1@gmail.com>")]
struct Opts {
    lang: Lang,
    path: String,
    out: String,
}

#[derive(Clone, Copy, Debug)]
enum Lang {
    Rust,
    TS,
}
impl std::str::FromStr for Lang {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust" => Ok(Lang::Rust),
            "ts" => Ok(Lang::TS),
            s => {
                println!("{}", s);
                Err("no match")
            }
        }
    }
}

fn extension(lang: Lang) -> &'static str {
    match lang {
        Lang::Rust => "rs",
        Lang::TS => "ts",
    }
}

fn visit_files(dir: &str, cb: &mut dyn FnMut(&fs::DirEntry) -> Result<()>) -> Result<()> {
    if fs::metadata(dir)?.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_files(path.to_str().unwrap(), cb)?;
            } else if path.extension().is_some() && path.extension().unwrap() == "pkt" {
                cb(&entry)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Schema {
    path: PathBuf,
    generated: String,
}

fn run_one(path: String, lang: Lang) -> Result<Schema> {
    Ok(Schema {
        path: PathBuf::from(path.clone()),
        generated: match lang {
            Lang::Rust => pkt::compile::<pkt::gen::rust::Rust>(&fs::read_to_string(path)?)?,
            Lang::TS => pkt::compile::<pkt::gen::ts::TypeScript>(&fs::read_to_string(path)?)?,
            //_ => return println!("not implemented"),
        },
    })
}

fn run_all(dir: String, lang: Lang) -> Result<Vec<Schema>> {
    let mut out = Vec::new();
    visit_files(&dir, &mut |entry| {
        out.push(run_one(
            entry
                .path()
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid path"))?
                .into(),
            lang,
        )?);
        Ok(())
    })?;
    Ok(out)
}

fn save_one(schema: Schema, out: &Path) -> Result<()> {
    println!("Writing to {}", out.display());
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, schema.generated)?;
    Ok(())
}

fn format_path(file: &Path, base_dir: &Path, out_dir: &Path, lang: Lang, preserve_dir: bool) -> Result<PathBuf> {
    if preserve_dir {
        Ok(out_dir
            .components()
            .chain(file.strip_prefix(base_dir)?.components())
            .collect::<PathBuf>()
            .with_extension(extension(lang)))
    } else {
        println!("{}", out_dir.display());
        Ok(out_dir
            .components()
            // This should never panic
            .chain(vec![Component::Normal(file.file_stem().unwrap())].into_iter())
            .collect::<PathBuf>()
            .with_extension(extension(lang)))
    }
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let base_dir = PathBuf::from(opts.path.clone());
    let out_dir = PathBuf::from(opts.out.clone());
    if let Ok(meta) = fs::metadata(&opts.path) {
        if meta.is_dir() {
            for result in run_all(opts.path.clone(), opts.lang)? {
                let out = result.path.clone();
                save_one(result, &format_path(&out, &base_dir, &out_dir, opts.lang, true)?)?;
            }
        } else {
            let result = run_one(opts.path.clone(), opts.lang)?;
            let out = result.path.clone();
            save_one(result, &format_path(&out, &base_dir, &out_dir, opts.lang, false)?)?;
        }
    }

    Ok(())
}
