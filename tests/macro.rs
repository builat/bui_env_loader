use crate::macro_config::{AppConfig, AppConfigSnapshot};
use bui_env_loader::{Event, Loaded, Loader, MaterializationMode};
mod macro_config;

#[test]
fn simple_macro_test_with_single_dotenv() -> Result<(), Box<dyn std::error::Error>> {
    let config = Loader::new()
        .add_dotenv("./tests/.env.test")
        .prefix("APP_")
        .reporter(|event: &Event<'_>| {
            eprintln!("{:?} {:?}: {}", event.level, event.kind, event.message,);
        })
        .load::<AppConfig>()?;

    // regular non-lazy fields
    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 8080);
    assert_eq!(config.workers, 4);

    // Optional eager-fields.
    assert!(config.token.is_some());

    assert_eq!(config.log_level.get()?, "debug");

    assert_eq!(config.public_url.get()?, "http://127.0.0.1:8080");
    Ok(())
}

#[test]
fn simple_macro_test_with_multiple_dotenvs() -> Result<(), Box<dyn std::error::Error>> {
    let config: Loaded<AppConfig> = Loader::new()
        .add_dotenv("./tests/.env.test")
        .add_dotenv_optional("./tests/.env.test_extra")
        .prefix("APP_")
        .reporter(|event: &Event<'_>| {
            eprintln!("{:?} {:?}: {}", event.level, event.kind, event.message,);
        })
        .load()?;

    let materialized = config.materialize(MaterializationMode::Warn)?;

    let snapshot: AppConfigSnapshot = materialized.value;

    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 8080);
    assert_eq!(config.workers, 5);
    assert_eq!(config.log_level.get()?, "debug");

    assert_eq!(snapshot.public_url, "http://127.0.0.1:8080");
    assert_eq!(
        snapshot.metrics_endpoint,
        Some("http://127.0.0.1:8080/metrics".to_string())
    );
    assert_eq!(snapshot.token, Some("development-token".to_string()));
    assert_eq!(
        snapshot.metrics_endpoint,
        config.metrics_endpoint.lazy_get_optional()?
    );

    for issue in materialized.report.issues() {
        println!("issue: {:?}, variable: {:?}", issue.kind, issue.variable,);
    }

    Ok(())
}
