trait FamilyGate {
    fn accept(&self, value: usize) -> Result<usize, String>;
}

pub fn support_trait_alpha(gate: &dyn FamilyGate, value: usize) -> Result<usize, String> {
    gate.accept(value)?;
    Ok(value + 1)
}

pub fn support_trait_beta(gate: &dyn FamilyGate, value: usize) -> Result<usize, String> {
    gate.accept(value)?;
    Ok(value + 1)
}

pub fn support_trait_gamma(gate: &dyn FamilyGate, value: usize) -> Result<usize, String> {
    gate.accept(value)?;
    Ok(value + 1)
}
