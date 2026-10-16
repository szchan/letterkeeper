// SPDX-License-Identifier: GPL-3.0-only
#[path = "../benches/support/entry.rs"]
mod entry;
use std::{ffi::OsString, path::PathBuf};

fn arguments(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn benchmark_requires_explicit_binary_and_output() {
    for args in [
        vec![],
        arguments(&["--binary", "app"]),
        arguments(&["--output", "result.json"]),
        arguments(&["--binary", "app", "--output", "out", "--binary", "other"]),
        arguments(&["--binary", "app", "--output", "out", "--unknown"]),
    ] {
        assert!(entry::Options::parse(&args).is_err());
    }
    let options =
        entry::Options::parse(&arguments(&["--binary", "app", "--output", "result.json"])).unwrap();
    assert_eq!(options.binary, PathBuf::from("app"));
    assert_eq!(options.output, PathBuf::from("result.json"));
}

#[test]
fn benchmark_measurement_passes_paths_and_preserves_failure() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("synthetic app");
    std::fs::write(&binary, b"synthetic binary placeholder").unwrap();
    let script = dir.path().join("measure.py");
    std::fs::write(&script, "import argparse, pathlib\np=argparse.ArgumentParser()\np.add_argument('--binary', required=True)\np.add_argument('--output', required=True)\np.add_argument('--driver')\na=p.parse_args()\nassert pathlib.Path(a.binary).is_file()\npathlib.Path(a.output).write_text('{\"synthetic\":true}')\n").unwrap();
    let options = entry::Options {
        binary,
        output: dir.path().join("result.json"),
    };
    entry::measure(&script, &options, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(&options.output).unwrap(),
        "{\"synthetic\":true}"
    );
    std::fs::write(&script, "raise SystemExit(7)\n").unwrap();
    assert!(entry::measure(&script, &options, None).is_err());
}
