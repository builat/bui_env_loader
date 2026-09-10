# bui_env_loader

Ещё один загрузчик конфигурации из переменных окружения. Написан на Rust и без зависимостей,
потому что тащить зависимость ради чтения
env-переменных — это как заказывать доставку пиццы в ресторан, где ты работаешь
поваром.

Дисклеймер: это сайд-проект. Он написан потому, что я устал
копипастить один и тот же код загрузки конфига из проекта в проект, как
герой мультфильма, который ничему не учится. Теперь копипаста живёт
в одном крейте, у неё есть тесты, и теперь это как бе софт.

[Английская версия README находится здесь](README.md)

## Что он умеет, если коротко

Описываем структуру конфига, указываем `Loader` на источники, и получаем
строго типизированные значения. Если длинно — смотрите ниже.

- Строгая типизация через обычный `FromStr`. Никакой рефлексии, никаких
  proc-макросов которые замедляют CI сильнее чем тесты, никакого serde.
  Поле типа `u16`, которому пришло `eighty` кастится в ошибку, а не подменяется в 0. В этом
  вообще вся суть.
- Источники с детерминированным приоритетом (побеждает первый найденный):
  1. явные значения из `Loader::set`;
  2. кастомные источники, в порядке регистрации;
  3. одноразовый снапшот переменных процесса;
  4. dotenv-файлы, в порядке регистрации.
- Один и тот же ключ в нескольких источниках: побеждает
  источник с наивысшим приоритетом, остальное попадает в отчёт как issue
  duplicate-variable.
- Три вида полей:
  - жадные (`get`, `get_optional`, `get_or`) — читаются сейчас, падают сейчас;
  - ленивые (`lazy_get`, `lazy_get_optional`) — читаются при каждом `.get()`,
    так что долгоживущий процесс видит изменившийся источник без танцев с
    бубном и релоадом;
  - вычисляемые (`get_computed`) — замыкание, которое получает resolve-сессию,
    может комбинировать другие переменные, нужно для всякого тяжелого или тогда когда понадобится
    можно впихуить свои ошибки, упадёт тогда когда значение будет затребовано.
- Материализация: превращение рантайм-конфига в полностью владеемый снапшот
  есть 4 режима  `Normal`, `Notify`, `Warn`, `Strict`. Strict
  отклоняет снапшот если вообще хоть что-то не так, что прекрасно для прода и
  и не подходит для дева.
- Аудит необъявленных переменных с ограничением по префиксу, чтобы `PATH` и
  `HOME` не считались подозрительной конфигурацией.
- Трейт `Reporter` вместо зависимости от логгера. События структурные, без
  значений (секреты не утекают by construction), их можно руками впихуить в `log`,
  `tracing` или `eprintln`.
- Ошибки категоризированы (`ConfigErrorKind`), несут контекст
  ключ/источник/ожидаемый-тип и не хранят значение — по той же
  причине про секреты. Я пару раз от этого страдал, больше не хочется.
- Декларативный макрос `env_config!`, который генерирует рантайм-структуру,
  структуру-снапшот и обе реализации трейтов — если писать их руками для вас
  слишком 2008-й год.

## Cargo.toml

```toml
[dependencies]
bui_env_loader = "0.1"
```

Только стандартная библиотека: в ваше дерево добавляется ровно один крейт и
ноль — в граф депсов. Это приятно, во всяком случае для меня.

## Как пользоваться

### Способ с макросом

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

Потом где-нибудь в коде. Скорее всего в `main`:

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

Обязательные статические значения:

```rust
let host = config.host; // Loaded<T> deref-ится в T, поля просто доступны
```

Ленивые значения (каждый `.get()` — новый поиск в источниках):

```rust
let log_level = config.log_level.get()?;
let endpoint = config.metrics_endpoint.lazy_get_optional()?;
```

Вычисляемые:

```rust
let public_url = config.public_url.get()?;
```

### Ручной способ

Макрос — это сахар. Под ним два обычных трейта, которые можно реализовать
самостоятельно и наслаждаться полным контролем, или страдать, зависит от дня:

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
        // Сначала разрешаем все поля, потом проверяем. Ранний `?` здесь
        // запрещён именно затем, чтобы Strict-режим собрал все проблемы в
        // один агрегированный отчёт.
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

### Режимы материализации

```rust
let materialized = config.materialize(MaterializationMode::Warn)?;
println!("issue(s): {}", materialized.report.issues().len());
```

| Режим   | Что делает                                                        |
|---------|-------------------------------------------------------------------|
| `Normal`| молчит, фатальные ошибки всё равно отклоняют снапшот              |
| `Notify`| `Normal` плюс уведомления о missing-optional на уровне Info       |
| `Warn`  | рапортует все расхождения, снапшот всё равно возвращается        |
| `Strict`| рапортует все расхождения и отклоняет снапшот при любом issue     |

Отчёт материализации — это список категоризированных issue (`IssueKind`:
missing optional/required, default used, unknown variable, duplicate
variable, invalid value, computation failed, source failed, internal), у
каждого есть severity (`Notice`, `Warning`, `Error`), имя поля, имя
переменной и источник который её принёс. Хватит на приличный стартовый
баннер.

### Кастомные источники

Всё что умеет ответить на вопрос «дай мне этот ключ» — уже источник:

```rust
use bui_env_loader::source::{MapSource, Source, SourceError};

let source = MapSource::new("test") // также удобно для тестов
    .with("APP_HOST", "localhost")
    .with("APP_PORT", "8080");

struct Vault; // представьте тут config-сервер или hashicorp vault

impl Source for Vault {
    fn name(&self) -> &str { "vault" }

    fn get(&self, key: &str) -> Result<Option<String>, SourceError> {
        match key {
            "APP_TOKEN" => Ok(Some("секрет".into())),
            _ => Ok(None),
        }
    }

    fn keys(&self) -> Result<Option<Vec<String>>, SourceError> {
        Ok(Some(vec!["APP_TOKEN".into()]))
    }
}

let config = Loader::new()
    .without_process_environment() // или оставьте, он ниже кастомных источников
    .source(Vault)
    .source(source)
    .load::<AppConfig>()?;
```

`MapSource` нормализует ключи к верхнему регистру, `ProcessEnvSource` —
неизменяемый UTF-8 снапшот, dotenv-файлы поддерживают `export`, кавычки,
стандартные эскейпы и комментарии. Это вся грамматика. Шелл-интерполяция и
многострочные значения не поддерживаются намеренно, потому что dotenv-файл в
котором завёлся шелл — это то, с чего начинаются фильмы ужасов.

### Ошибки

```rust
match Loader::new().load::<AppConfig>() {
    Err(error) => match error.kind() {
        ConfigErrorKind::MissingVariable => /* сказать оператору какой ключ, error.key() знает */,
        ConfigErrorKind::InvalidValue    => /* error.expected_type() знает тип */,
        ConfigErrorKind::DotenvSyntax    => /* номер строки есть в сообщении */,
        _ => /* Io, Source, Computation, InvalidKey, Internal */,
    },
    Ok(config) => { /* работаем дальше */ }
}
```

Ошибки реализуют `std::error::Error` и дружат с `?`. Сырое значение переменной
никогда не хранится в ошибке и не попадает в события репортера, так что
встройка диагностики в ваши логи не может опубликовать пароль от базы. Пара
оттуда утечёт каким-нибудь другим способом, почти наверняка, но не этим.

## Примеры

На каждую фичу есть запускаемый пример, без отговорок:

```bash
cargo run --example basic                    # ручные EnvConfig + Materialize, всё целиком
cargo run --example macro_config             # макрос env_config! генерирует обе формы
cargo run --example eager_fields             # get / get_optional / get_or со std-типами
cargo run --example deferred_fields          # Lazy, LazyOptional, Computed перечитывания
cargo run --example sources_and_precedence   # приоритет источников, кастомные, дубликаты
cargo run --example dotenv_layering          # обязательные/опциональные dotenv и слои
cargo run --example process_environment      # снапшот env, аудит префикса, отключение
cargo run --example reporter                 # кастомный Reporter с состоянием
cargo run --example materialization_modes    # сравнение Normal / Notify / Warn / Strict
cargo run --example error_handling           # матчинг ConfigErrorKind, безопасные сообщения
```

## Тесты

```bash
cargo test
```

На момент написания — 73 теста. Они покрывают приоритет источников, грамматику
dotenv и способы её сломать, оба встроенных источника, репортер, все режимы
материализации, дедупликацию отчётов и страховку, которая ловит реализацию
`Materialize`, возвращающую `None` без записанной ошибки. Если вы что-то
сломаете — по крайней мере узнаете об этом раньше продакшена. Наверное.

## Ограничения

- Только стандартная библиотека. Это дизайн-ограничение, все это дело неплохо упрощает аудит проекта.
- Жадные поля останавливаются на первой фатальной ошибке во время `load`.
  Ошибки отложенных полей агрегируются позже, при материализации. Декларативная
  схема могла бы убрать эту асимметрию в будущем, не меняя дизайн
  источник/резолвер.
- Async в вычисляемых полях не поддерживается. Может когда-то появится, если я или кто-то когда-то сделает.
- `rust-version = 1.98.0`. Будущее уже здесь.

## Лицензия

MIT. Делайте что хотите, только не вините меня.
