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

        public_url: String =
            get_computed(
                "public_url",
                |session: &mut ResolveSession| {
                    let host =
                        session.get::<String>("APP_HOST")?;

                    let port =
                        session.get::<u16>("APP_PORT")?;

                    Ok(format!("http://{host}:{port}"))
                }
            );
    }
}
