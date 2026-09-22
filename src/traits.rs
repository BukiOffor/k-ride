pub trait Validate {
    fn validate(&self) -> Result<(), crate::errors::ModuleError>;
}
