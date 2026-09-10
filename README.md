# bstrapio / bui_env_loader

Yet another environment configuration loader. Written in Rust, depends on
exactly nothing, because adding a dependency to read env vars felt a bit like
ordering pizza delivery to the restaurant you work at.

The honesty section: this is a side project. It was written because I kept
copy-pasting the same config-loading code between projects like some kind of
cartoon character who never learns. Now the copy-paste lives in one crate and
has tests, so it is officially Software.

[README on russian is here](README.ru.md)

## What it actually do

Short version: you describe a config struct, point a `Loader` at some sources,
and get strongly typed values back. Long version below, unfortunatelly.

- Strongly typed fields via plain `FromStr`. No reflection, no proc macros that
  rerun your CI slower than your tests, no serde. A `u16` field that receives
  `eighty` is an error, not a zero. This is the whole point.
- Sources with deterministic precedence (first match wins):
  1. explicit values from `Loader::set`
  2. custom sources, in registration order
  3. a one-time snapshot of the process environment
  4. dotenv files, in registration order
- The same key found in several places is not a mystery: the highest priority
  source wins and the rest is reported as a duplicate-variable issue.
- Three kinds of fields:
  - eager (`get`, `get_optional`, `get_or`) — read now, fail now;
  - lazy (`lazy_get`, `lazy_get_optional`) — read on every `.get()`, so a long
    running process can observe a changed source without reload ceremony;
  - computed (`get_computed`) — a closure that gets a resolve session and can
    combine other variables, be expensive, or fail with its own error kind.
- Materialization: turn the runtime config into a fully owned snapshot in one
  of four modes — `Normal`, `Notify`, `Warn`, `Strict`. Strict rejects the
  snapshot if literally anything is off, which is nice for production and
  unbearable for local hacking. Your choice.
- Unknown-variable auditing scoped by prefix, so `PATH` and `HOME` do not get
  flagged as suspicious config.
- Reporter trait instead of a logging dependency. Events are structured,
  value-free (secrets do not leak by construction), and can be bridged to
  `log`, `tracing`, or `eprintln`, the best logging framework.
- Errors are categorized (`ConfigErrorKind`), carry key/source/expected-type
  context, and never store the raw value, for the same secret-leaking reason.
- `env_config!` declarative macro that generates the runtime struct, the
  snapshot struct, and both trait impls, if writing them by hand feels too
  2008 for you.

## Cargo.toml

```toml
[dependencies]
bui_env_loader = "0.1"
```

Std-only, so this adds exactly one crate to your tree and zero to your
dependency graph. Depending on your feelings about transitive dependencies,
this is either nice or very nice.

## How to use

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
                    let host = session.get::<String>("APP_HOST")?;
                    let port = session.get::<u16>("APP_PORT")?;
                    Ok(format!("http://{host}:{port}"))
                }
            );
    }
}
```

Then somewhere in the code. Probably in `main`:

```rust
let config: Loaded<AppConfig> = Loader::new()
    .add_dotenv(".env")
    .add_dotenv_optional(".env.local")
    .prefix("APP_")
    .reporter(|event: &Event<'_>| {
        eprintln!("{:?} {:?}: {}", event.level, event.kind, event.message);
    })
    .load()?;
```

For mandatory static values:

```rust
let host = config.host; // Loaded<T> derefs to T, so fields are just there
```

For lazy values (each `.get()` is a fresh lookup):

```rust
let log_level = config.log_level.get()?;
let endpoint = config.metrics_endpoint.lazy_get_optional()?;
```

For the computed:

```rust
let public_url = config.public_url.get()?;
```

### Hand-written way

The macro is sugar. Under it there is two ordinary traits, and you can
implement them yourself and enjoy full control, or suffer, depending on the
day:

```rust
use bui_env_loader::{
    Computed, ConfigContext, ConfigError, EnvConfig, Lazy, MaterializationContext,
    Materialize, ResolveSession,
};

struct AppConfig {
    port: u16,
    log_level: Lazy<String>,
    public_url: Computed<String>,
}

impl EnvConfig for AppConfig {
    fn from_env(context: &mut ConfigContext) -> Result<Self, ConfigError> {
        Ok(Self {
            port: context.get("APP_PORT")?,
            log_level: context.lazy_get("APP_LOG_LEVEL")?,
            public_url: context.get_computed("public_url", |session: &mut ResolveSession| {
                let port = session.get::<u16>("APP_PORT")?;
                Ok(format!("http://localhost:{port}"))
            }),
        })
    }
}

struct AppConfigSnapshot {
    port: u16,
    log_level: String,
    public_url: String,
}

impl Materialize for AppConfig {
    type Output = AppConfigSnapshot;

    fn materialize(&self, context: &mut MaterializationContext<'_>) -> Option<Self::Output> {
        // Resolve everything first, check later. No early `?` here is what
        // lets Strict mode report every problem in one aggregate report.
        let log_level = context.lazy("log_level", &self.log_level);
        let public_url = context.computed("public_url", &self.public_url);

        match (log_level, public_url) {
            (Some(log_level), Some(public_url)) => Some(AppConfigSnapshot {
                port: self.port,
                log_level,
                public_url,
            }),
            _ => None,
        }
    }
}
```

### Materialization modes

```rust
let materialized = config.materialize(MaterializationMode::Warn)?;
println!("{} issue(s)", materialized.report.issues().len());
```

| Mode     | What it do                                                      |
|----------|-----------------------------------------------------------------|
| `Normal` | silent, fatal errors still reject                               |
| `Notify` | `Normal` plus missing-optional notices at Info level            |
| `Warn`   | reports every discrepancy, snapshot is still returned          |
| `Strict` | reports every discrepancy and rejects if any issue exists      |

A materialization report is a list of categorized issues (`IssueKind`:
missing optional/required, default used, unknown variable, duplicate
variable, invalid value, computation failed, source failed, internal), each
with a severity (`Notice`, `Warning`, `Error`), a field name, a variable name
and the source that caused it. Enough to build a decent startup banner.

### Custom sources

Anything that can answer "give me this key" is a source:

```rust
use bui_env_loader::source::{MapSource, Source, SourceError};

let source = MapSource::new("test") // also handy for tests
    .with("APP_HOST", "localhost")
    .with("APP_PORT", "8080");

struct Vault; // imagine a config server or hashicorp vault here

impl Source for Vault {
    fn name(&self) -> &str { "vault" }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        match key {
            "APP_TOKEN" => Ok(Some("sekret".into())),
            _ => Ok(None),
        }
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(vec!["APP_TOKEN".into()]))
    }
}

let config = Loader::new()
    .without_process_environment() // or keep it, it sits below custom sources
    .source(Vault)
    .source(source)
    .load::<AppConfig>()?;
```

`MapSource` normalizes keys to upper case, `ProcessEnvSource` is an immutable
UTF-8 snapshot, dotenv files support `export`, quotes, common escapes and
comments, and that is the whole grammar. Shell interpolation and multiline
values are not supported, on purpose, because dotenv files that grew a shell
inside are how horror movies start.

### Errors

```rust
match Loader::new().load::<AppConfig>() {
    Err(error) => match error.kind() {
        ConfigErrorKind::MissingVariable => /* tell the operator which key, error.key() knows */,
        ConfigErrorKind::InvalidValue    => /* error.expected_type() knows the type */,
        ConfigErrorKind::DotenvSyntax    => /* line number is in the message */,
        _ => /* Io, Source, Computation, InvalidKey, Internal */,
    },
    Ok(config) => { /* proceed */ }
}
```

Errors implement `std::error::Error` and compose with `?`. The raw variable
value is never stored in an error and never appears in reporter events, so
wiring diagnostics into your logs can not publish your database password. You
would manage to publish it some other way, probably, but not this way.

## Examples

Every feature has a runnable example, no excuses:

```bash
cargo run --example basic                    # hand-written EnvConfig + Materialize, end to end
cargo run --example macro_config             # the env_config! macro generating both forms
cargo run --example eager_fields             # get / get_optional / get_or with std types
cargo run --example deferred_fields          # Lazy, LazyOptional, Computed re-reads
cargo run --example sources_and_precedence   # source priority, custom sources, duplicates
cargo run --example dotenv_layering          # required/optional dotenv files and layering
cargo run --example process_environment      # env snapshot, prefix auditing, opting out
cargo run --example reporter                 # custom Reporter with state and filtering
cargo run --example materialization_modes    # Normal / Notify / Warn / Strict compared
cargo run --example error_handling           # matching ConfigErrorKind, safe messages
```

## Tests

```bash
cargo test
```

73 tests at the time of this writing. They cover source precedence, dotenv
grammar and its failure modes, both builtin sources, the reporter, all
materialization modes, report deduplication, and the guardrail that catches a
`Materialize` impl returning `None` without recording an error. If you break
something, at least you will find out before production does. Probably.

## Constraints, honestly

- Standard library only. This is a design constraint, not a virtue signal, but
  it does mean the audit surface is small.
- Eager fields stop at the first fatal error during `load`. Deferred fields
  are aggregated at materialization instead. A declarative schema could remove
  this asymmetry later without changing the source/resolver design.
- Async is not supported in computed fields. It could be, someday, by someone.
- `rust-version = 1.98.0`. The future is now, apparently.

## License

MIT. Do whatever, just do not blame me.
