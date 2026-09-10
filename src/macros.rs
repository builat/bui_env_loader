/// Creates runtime- and snapshot-types along with necessary trait implementations.
///
/// Type selected after the `=` sign determines the type of the runtime field and the type of the snapshot field.
///
/// - `get_optional`        -> `Option<T>`;
/// - `lazy_get`            -> `Lazy<T>`;
/// - `lazy_get_optional`   -> `LazyOptional<T>`;
/// - `get_computed`        -> `Computed<T>`.
///
/// Example:
///
/// ```ignore
/// token: String = get_optional("APP_TOKEN");
/// ```
///
/// Runtime field will have type `Option<String>`. Snapshot field will also have type   
/// `Option<String>`.
#[macro_export]
// ---------------------------------------------------------------------
// Main macro part for generating runtime and snapshot types along with trait implementations.
// / ---------------------------------------------------------------------
macro_rules! env_config {
    (
        $vis:vis config $config:ident => $snapshot:ident {
            $(
                $field:ident: $field_type:ty =
                    $kind:ident($($arguments:tt)*);
            )*
        }
    ) => {
        // -------------------------------------------------------------
        // 1. Runtime-structures.
        //
        // The type of the field depends on the way it is retrieved from the source.:
        //
        // get              -> T
        // get_optional     -> Option<T>
        // get_or           -> T
        // lazy_get         -> Lazy<T>
        // lazy_get_optional -> LazyOptional<T>
        // get_computed     -> Computed<T>
        // -------------------------------------------------------------
        $vis struct $config {
            $(
                pub $field: $crate::env_config!(
                    @runtime_type
                    $kind,
                    $field_type
                ),
            )*
        }

        // -------------------------------------------------------------
        // 2. fully materialized snapshot.
        //
        // Here Lazy<T> and Computed<T> are fully materialized.
        // -------------------------------------------------------------
        $vis struct $snapshot {
            $(
                pub $field: $crate::env_config!(
                    @snapshot_type
                    $kind,
                    $field_type
                ),
            )*
        }

        // -------------------------------------------------------------
        // 3. Creating runtime configuration.
        // -------------------------------------------------------------
        impl $crate::EnvConfig for $config {
            fn from_env(
                context: &mut $crate::ConfigContext,
            ) -> Result<Self, $crate::ConfigError> {
                // NOTE: no "?" operator is used here to avoid early return on the first error.
                //
                // Executing everything in one pass allows us to collect all errors and report them at once.
                // Probably this is not the smartest way to do it, but it is the simplest and most straightforward.
                $(
                    let $field = $crate::env_config!(
                        @initialize
                        context,
                        $kind,
                        $field_type,
                        $($arguments)*
                    );
                )*

                // Current EnvConfig will return a single ConfigError.
                // So we need to check all results and return the first error if any.
                // Still this is NOT THE SMARTEST WAY
                let mut __first_error:
                    Option<$crate::ConfigError> = None;

                $(
                    if let Err(error) = &$field {
                        if __first_error.is_none() {
                            __first_error = Some(error.clone());
                        }
                    }
                )*

                if let Some(error) = __first_error {
                    return Err(error);
                }

                // All results are checked so we can not face Err here.
                Ok(Self {
                    $(
                        $field: match $field {
                            Ok(value) => value,
                            Err(_) => unreachable!(
                                "all configuration errors were checked above"
                            ),
                        },
                    )*
                })
            }
        }

        // -------------------------------------------------------------
        // 4. Converting runtime configuration into fully materialized snapshot.
        // -------------------------------------------------------------
        impl $crate::Materialize for $config {
            type Output = $snapshot;

            fn materialize(
                &self,
                context: &mut $crate::MaterializationContext<'_>,
            ) -> Option<Self::Output> {
                // Materializing all the fields in one pass allows us to collect all errors and report them at once.
                // So we can collect all the errors of several lazyy/computed fields in one call to materialize.
                $(
                    let $field = $crate::env_config!(
                        @materialize_field
                        context,
                        self,
                        $field,
                        $kind
                    );
                )*

                // Each intermediate result has the form Option<T>.
                // For optional fields this is Option<Option<T>>:
                //
                // - the outer Option indicates a resolution error;
                // - the inner Option indicates the absence of an optional value. :D
                match (
                    $(
                        $field,
                    )*
                ) {
                    (
                        $(
                            Some($field),
                        )*
                    ) => {
                        Some($snapshot {
                            $(
                                $field,
                            )*
                        })
                    }

                    _ => None,
                }
            }
        }
    };

    // =====================================================================
    // Runtime-types
    // =====================================================================

    (@runtime_type get, $field_type:ty) => {
        $field_type
    };

    (@runtime_type get_optional, $field_type:ty) => {
        Option<$field_type>
    };

    (@runtime_type get_or, $field_type:ty) => {
        $field_type
    };

    (@runtime_type lazy_get, $field_type:ty) => {
        $crate::Lazy<$field_type>
    };

    (@runtime_type lazy_get_optional, $field_type:ty) => {
        $crate::LazyOptional<$field_type>
    };

    (@runtime_type get_computed, $field_type:ty) => {
        $crate::Computed<$field_type>
    };

    // =====================================================================
    // Types of materialized snapshot
    // =====================================================================

    (@snapshot_type get, $field_type:ty) => {
        $field_type
    };

    (@snapshot_type get_optional, $field_type:ty) => {
        Option<$field_type>
    };

    (@snapshot_type get_or, $field_type:ty) => {
        $field_type
    };

    (@snapshot_type lazy_get, $field_type:ty) => {
        $field_type
    };

    (@snapshot_type lazy_get_optional, $field_type:ty) => {
        Option<$field_type>
    };

    (@snapshot_type get_computed, $field_type:ty) => {
        $field_type
    };

    // =====================================================================
    // Init of runtime-fields
    // =====================================================================

    (
        @initialize
        $context:ident,
        get,
        $field_type:ty,
        $key:literal
    ) => {
        $context.get::<$field_type>($key)
    };

    (
        @initialize
        $context:ident,
        get_optional,
        $field_type:ty,
        $key:literal
    ) => {
        $context.get_optional::<$field_type>($key)
    };

    (
        @initialize
        $context:ident,
        get_or,
        $field_type:ty,
        $key:literal,
        $default:expr
    ) => {
        $context.get_or::<$field_type>($key, $default)
    };

    (
        @initialize
        $context:ident,
        lazy_get,
        $field_type:ty,
        $key:literal
    ) => {
        $context.lazy_get::<$field_type>($key)
    };

    (
        @initialize
        $context:ident,
        lazy_get_optional,
        $field_type:ty,
        $key:literal
    ) => {
        $context.lazy_get_optional::<$field_type>($key)
    };

    (
        @initialize
        $context:ident,
        get_computed,
        $field_type:ty,
        $name:literal,
        $function:expr
    ) => {
        Ok::<$crate::Computed<$field_type>, $crate::ConfigError>(
            $context.get_computed::<$field_type, _>(
                $name,
                $function,
            )
        )
    };

    // =====================================================================
    // Materialization of runtime-fields
    // =====================================================================

    (
        @materialize_field
        $context:ident,
        $self_value:ident,
        $field:ident,
        get
    ) => {
        Some($self_value.$field.clone())
    };

    (
        @materialize_field
        $context:ident,
        $self_value:ident,
        $field:ident,
        get_optional
    ) => {
        Some($self_value.$field.clone())
    };

    (
        @materialize_field
        $context:ident,
        $self_value:ident,
        $field:ident,
        get_or
    ) => {
        Some($self_value.$field.clone())
    };

    (
        @materialize_field
        $context:ident,
        $self_value:ident,
        $field:ident,
        lazy_get
    ) => {
        $context.lazy(
            stringify!($field),
            &$self_value.$field,
        )
    };

    (
        @materialize_field
        $context:ident,
        $self_value:ident,
        $field:ident,
        lazy_get_optional
    ) => {
        $context.lazy_optional(
            stringify!($field),
            &$self_value.$field,
        )
    };

    (
        @materialize_field
        $context:ident,
        $self_value:ident,
        $field:ident,
        get_computed
    ) => {
        $context.computed(
            stringify!($field),
            &$self_value.$field,
        )
    };
}
