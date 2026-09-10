use bui_env_loader::{
    source::{MapSource, Source},
    Loader,
};
mod macro_config;

#[test]
fn materializes_test_config() {
    let source = MapSource::new("test")
        .with("APP_HOST", "localhost")
        .with("APP_PORT", "3000")
        .with("APP_LOG_LEVEL", "info")
        .with("APP_WORKERS", "4");

    let check_source = source.clone();
    let k = check_source
        .keys()
        .expect("should contain keys")
        .expect("Should be non empty");

    let v = check_source
        .values()
        .expect("should contain values")
        .expect("Should be non empty");

    assert_eq!(k.len(), 4);
    assert_eq!(v.len(), 4);

    let config = Loader::new()
        .without_process_environment()
        .source(source)
        .load::<macro_config::AppConfig>()
        .expect("Configuration should load");

    let snapshot: bui_env_loader::Materialized<macro_config::AppConfigSnapshot> = config
        .materialize(bui_env_loader::MaterializationMode::Normal)
        .expect("Configuration should materialize");

    assert_eq!(snapshot.report.has_errors(), false);
    assert_eq!(snapshot.report.is_clean(), false);

    assert_eq!(snapshot.value.host, "localhost");
    assert_eq!(snapshot.value.port, 3000);
    assert_eq!(snapshot.value.public_url, "http://localhost:3000");
}
