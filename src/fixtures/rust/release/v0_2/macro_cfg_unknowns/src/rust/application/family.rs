macro_rules! generated_family_gate {
    ($name:ident) => {
        pub fn $name(value: usize) -> Result<usize, String> {
            Ok(value + 1)
        }
    };
}

generated_family_gate!(support_generated_alpha);
generated_family_gate!(support_generated_beta);
generated_family_gate!(support_generated_gamma);

#[cfg(feature = "generated")]
pub fn support_cfg_alpha(value: usize) -> Result<usize, String> {
    Ok(value + 1)
}

#[cfg_attr(feature = "generated", inline)]
pub fn support_cfg_beta(value: usize) -> Result<usize, String> {
    Ok(value + 1)
}

#[proc_macro_attribute]
pub fn support_proc_macro_alpha(value: usize) -> Result<usize, String> {
    Ok(value + 1)
}
