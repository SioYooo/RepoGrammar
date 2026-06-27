pub fn execute_install_alpha(action: &str) -> Result<String, String> {
    write_receipt(action)?;
    rollback_action(action)?;
    Ok(action.to_string())
}

pub fn execute_install_beta(action: &str) -> Result<String, String> {
    write_receipt(action)?;
    rollback_action(action)?;
    Ok(action.to_string())
}

pub fn execute_install_gamma(action: &str) -> Result<String, String> {
    write_receipt(action)?;
    rollback_action(action)?;
    Ok(action.to_string())
}

fn write_receipt(_action: &str) -> Result<(), String> {
    Ok(())
}

fn rollback_action(_action: &str) -> Result<(), String> {
    Ok(())
}
