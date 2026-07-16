pub fn support_low_alpha(value: usize) -> Result<usize, String> {
    validate_family(value)?;
    Ok(value + 1)
}

pub fn support_low_beta(value: usize) -> Result<usize, String> {
    validate_family(value)?;
    Ok(value + 1)
}

fn validate_family(value: usize) -> Result<(), String> {
    if value == 0 {
        return Err("zero is not accepted".to_string());
    }
    Ok(())
}
