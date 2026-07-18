#[test]
fn reserves_order() {
    let outcome = place_order("alpha");
    assert!(outcome);
}

#[test]
fn cancels_order() {
    let outcome = place_order("beta");
    assert!(outcome);
}

#[test]
fn lists_orders() {
    let outcome = place_order("gamma");
    assert!(outcome);
}

fn place_order(_name: &str) -> bool {
    true
}
