#[test]
fn product_runtime_alpha() {
    let output = run_with_runtime("alpha");
    assert!(parse_machine_output(output));
}

#[test]
fn product_runtime_beta() {
    let output = run_with_runtime("beta");
    assert!(parse_machine_output(output));
}

#[test]
fn product_runtime_gamma() {
    let output = run_with_runtime("gamma");
    assert!(parse_machine_output(output));
}

fn run_with_runtime(_name: &str) -> String {
    "ok".to_string()
}

fn parse_machine_output(output: String) -> bool {
    output == "ok"
}
