pub mod constants;
pub mod dns;
pub mod prelude;
pub mod structs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_setup() {
        assert!(true, "Basic test infrastructure is working");
    }
}
