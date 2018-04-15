extern crate temp_testdir;
#[macro_use]
extern crate rstest_util;

use temp_testdir::TempDir;

pub mod prj;
pub mod utils;

use utils::*;
use prj::Project;

#[test]
fn one_success_test() {
    let root = TempDir::default();
    let project = Project::new(&root).create();

    project.append_code(
        r#"
        #[test]
        fn success() {
            assert!(true);
        }
        "#
    );

    let output = project.run_tests().unwrap();

    TestResults::new()
        .ok("success")
        .assert(output);
}

#[test]
fn one_fail_test() {
    let root = TempDir::default();
    let project = Project::new(&root).create();

    project.append_code(
        r#"
        #[test]
        fn fail() {
            assert!(false);
        }
        "#
    );

    let output = project.run_tests().unwrap();

    TestResults::new()
        .fail("fail")
        .assert(output);
}

#[test]
fn parametrize_simple_should_compile() {
    let root = TempDir::default();
    let output = Project::new(&root)
        .create()
        .set_code_file(resources("parametrize_simple.rs"))
        .compile()
        .unwrap();

    assert_eq!(Some(0), output.status.code(), "Compile error due: {}", output.stderr.str())
}

fn run_test(res: &str) -> std::process::Output {
    let root = TempDir::default().permanent();
    Project::new(&root)
        .create()
        .set_code_file(resources(res))
        .run_tests()
        .unwrap()
}

#[test]
fn parametrize_simple_happy_path() {
    let output = run_test("parametrize_simple.rs");

    TestResults::new()
        .ok("strlen_test_case_0")
        .ok("strlen_test_case_1")
        .assert(output);
}

#[test]
fn parametrize_mut() {
    let output = run_test("parametrize_mut.rs");

    TestResults::new()
        .ok("add_test_case_0")
        .ok("add_test_case_1")
        .assert(output);
}


#[test]
fn parametrize_generic() {
    let output = run_test("parametrize_generic.rs");

    TestResults::new()
        .ok("strlen_test_case_0")
        .ok("strlen_test_case_1")
        .assert(output);
}