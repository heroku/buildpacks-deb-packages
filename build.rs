use std::env;
use std::fs::{read_to_string, write};
use std::path::PathBuf;
use std::str::FromStr;
use toml_edit::DocumentMut;

fn main() {
    println!("cargo:rerun-if-changed=buildpack.toml");

    let buildpack_contents = read_to_string("buildpack.toml").expect("buildpack.toml should exist");

    let doc = DocumentMut::from_str(&buildpack_contents)
        .expect("buildpack.toml should be a valid toml document");

    let buildpack = doc
        .get("buildpack")
        .and_then(|item| item.as_table_like())
        .expect("buildpack.toml should contain a [buildpack] table entry");

    let name = buildpack
        .get("name")
        .and_then(|item| item.as_str())
        .expect("buildpack.toml should contain an `id` field in the [buildpack] table entry");

    let version = buildpack
        .get("version")
        .and_then(|item| item.as_str())
        .expect("buildpack.toml should contain a `version` field in the [buildpack] table entry");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR should exist for a build script");

    let generated_buildpack_info = format!(
        r#"
const BUILDPACK_VERSION: &str = "{version}";
const BUILDPACK_NAME: &str = "{name}";
    "#
    );

    write(
        PathBuf::from(&out_dir).join("buildpack_info.rs"),
        generated_buildpack_info,
    )
    .expect("generated buildpack_info.rs should be written");
}
