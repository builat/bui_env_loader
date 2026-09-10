# Yet another env config loader as side project

### Why?
Well for no specific reason. I just wanted to created lib I always wanna to use and not reimplement code in each project.


## How to use?

### Macro way
```rust
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
```

And then somewhere in the code. Probably in `main`

```rust
    let config: Loaded<AppConfig> = Loader::new()
        .add_dotenv(".env")
        .add_dotenv_optional(".env.extra")
        .prefix("APP_")
        .reporter(|event: &Event<'_>| {
            eprintln!("{:?} {:?}: {}", event.level, event.kind, event.message,);
        })
        .load()?;
```

For mandatory static values: 

```rust
let host = config.host;
```

Fot optionsl static values:

```rust
let log_level = config.log_level.get()?;
```

Fot lazy optional values: 
```rust
let metric_endpoint = config.metrics_endpoint.lazy_get_optional()?;
```

For the computed: 
```rust
let public_url = config.public_url.get()?;
```