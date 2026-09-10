#[cfg(test)]
use bui_env_loader::ConfigError;
use bui_env_loader::ResolveSession;

#[cfg(test)]
bui_env_loader::env_config! {
    pub config AppConfig => AppConfigSnapshot {
        host: String =
            get("APP_HOST");

        port: u16 =
            get("APP_PORT");

        token: String =
            get_optional("APP_TOKEN");

        workers: usize =
            get_or("APP_WORKERS", 4);

        log_level: String =
            lazy_get("APP_LOG_LEVEL");


        metrics_endpoint: String =
            lazy_get_optional("APP_METRICS_ENDPOINT");

        bad_computed: String =
            get_computed(
                "bad_computed",
                |_: &mut ResolveSession| {
                    Err(ConfigError::computation(
                        "bad_computed",
                        "Failed to compute bad_computed due to missing APP_HOST or APP_PORT",
                    ))
                }
            );

    }
}
