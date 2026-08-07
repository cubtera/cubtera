use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PRG: &str = "cubtera";

// --------------------------------------------------
// fn run (args: &[&str], expected_file: &str) -> TestResult {
//     let expected = std::fs::read_to_string(expected_file)?;
//     Command::cargo_bin(PRG)?
//         .args(args)
//         .assert()
//         .success()
//         .stdout(expected);
//     Ok(())
// }
// // --------------------------------------------------
// fn run_stdin(
//     input_file: &str,
//     args: &[&str],
//     expected_file: &str,
// ) -> Result<()> {
//     let input = std::fs::read_to_string(input_file)?;
//     let expected = std::fs::read_to_string(expected_file)?;
//     Command::cargo_bin(PRG)?
//         .args(args)
//         .write_stdin(input)
//         .assert()
//         .success()
//         .stdout(expected);
//     Ok(())
// }

#[test]
fn tf_without_arguments() -> TestResult {
    Command::cargo_bin(PRG)?
        .args(["run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "the following required arguments were not provided:",
        ));
    Ok(())
}

#[test]
fn dies_no_args() -> TestResult {
    let mut cmd = Command::cargo_bin("cubtera")?;
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage: cubtera <COMMAND>"));
    Ok(())
}

#[test]
fn runs() {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    cmd.arg("--help").assert().success();
}
