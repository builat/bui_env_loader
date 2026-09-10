use crate::bad_macro_config::AppConfig;
use bui_env_loader::Loader;
mod bad_macro_config;

#[test]
fn loading_error_test() -> Result<(), Box<dyn std::error::Error>> {
    let config = Loader::new()
        .add_dotenv("./tests/.env.test_non_existent")
        .prefix("APP_")
        .load::<AppConfig>();
    assert!(config.is_err());
    let e = config.err().expect("Should have an error");
    assert_eq!(e.kind(), bui_env_loader::ConfigErrorKind::Io);
    Ok(())
}
#[test]
fn malformed_error_test() -> Result<(), Box<dyn std::error::Error>> {
    let config = Loader::new()
        .add_dotenv("./tests/.env.test_malformed")
        .prefix("APP_")
        .load::<AppConfig>();
    assert!(config.is_err());
    let e = config.err().expect("Should have an error");
    assert_eq!(e.kind(), bui_env_loader::ConfigErrorKind::DotenvSyntax);
    Ok(())
}

#[test]
fn invalid_value_error_test() -> Result<(), Box<dyn std::error::Error>> {
    let config = Loader::new()
        .add_dotenv("./tests/.env.test_bad")
        .prefix("APP_")
        .load::<AppConfig>();
    assert!(config.is_err());
    let e = config.err().expect("Should have an error");
    // The error should be a missing value for APP_WORKERS, which is required to materialize the config.
    assert_eq!(e.kind(), bui_env_loader::ConfigErrorKind::InvalidValue);
    Ok(())
}

#[test]
fn missing_value_error_test() -> Result<(), Box<dyn std::error::Error>> {
    let config = Loader::new()
        .prefix("APP_")
        .set("APP_PORT", "8080")
        .set("APP_TOKEN", "development-token")
        .load::<AppConfig>();
    assert!(config.is_err());
    let e = config.err().expect("Should have an error");
    assert_eq!(e.kind(), bui_env_loader::ConfigErrorKind::MissingVariable);
    Ok(())
}

#[test]
fn computation_error_test() -> Result<(), Box<dyn std::error::Error>> {
    let config = Loader::new()
        .add_dotenv("./tests/.env.test")
        .add_dotenv("./tests/.env.test_extra")
        .prefix("APP_")
        .load::<AppConfig>();

    let cmp = config?.bad_computed.get();
    assert!(cmp.is_err());
    let e = cmp.err().expect("Should have an error");
    assert_eq!(e.kind(), bui_env_loader::ConfigErrorKind::Computation);
    Ok(())
}
