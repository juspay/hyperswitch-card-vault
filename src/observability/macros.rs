/// Create a global [`Meter`][::opentelemetry::metrics::Meter] with the specified name.
#[macro_export]
macro_rules! global_meter {
    ($vis:vis $meter:ident, $name:literal) => {
        $vis static $meter: ::std::sync::LazyLock<::opentelemetry::metrics::Meter> =
            ::std::sync::LazyLock::new(|| {
                ::opentelemetry::global::meter($name)
            });
    };
    ($vis:vis $meter:ident) => {
        $vis static $meter: ::std::sync::LazyLock<::opentelemetry::metrics::Meter> =
            ::std::sync::LazyLock::new(|| {
                ::opentelemetry::global::meter(::std::stringify!($meter))
            });
    };
}

/// Create a [`Counter<u64>`][::opentelemetry::metrics::Counter] with the given name.
#[macro_export]
macro_rules! counter_metric {
    // Entry without `name:` override
    ($vis:vis $name:ident, $meter:ident $($rest:tt)*) => {
        $crate::counter_metric! { @build $vis $name, $meter [$($rest)*]
            $meter.u64_counter(::std::stringify!($name)) }
    };

    // Entry with `name:` override
    (@build $vis:vis $name:ident, $meter:ident
        [name: $metric_name:literal $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::counter_metric! { @build $vis $name, $meter [$($rest)*]
            $meter.u64_counter($metric_name) }
    };

    // `description:` keyword
    (@build $vis:vis $name:ident, $meter:ident
        [description: $v:literal $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::counter_metric! { @build $vis $name, $meter [$($rest)*]
            $($expr)* .with_description($v) }
    };

    // `unit:` keyword
    (@build $vis:vis $name:ident, $meter:ident
        [unit: $v:literal $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::counter_metric! { @build $vis $name, $meter [$($rest)*]
            $($expr)* .with_unit($v) }
    };

    // Comma skimmer
    (@build $vis:vis $name:ident, $meter:ident
        [, $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::counter_metric! { @build $vis $name, $meter [$($rest)*]
            $($expr)* }
    };

    // Base case: no more tokens, emit counter
    (@build $vis:vis $name:ident, $meter:ident [] $($expr:tt)*) => {
        $vis static $name: ::std::sync::LazyLock<
            ::opentelemetry::metrics::Counter<u64>
        > = ::std::sync::LazyLock::new(|| { $($expr)* .build() });
    };
}

/// Create a [`Histogram<f64>`][::opentelemetry::metrics::Histogram] with the given name.
#[macro_export]
macro_rules! histogram_metric_f64 {
    // Entry without `name:` override
    ($vis:vis $name:ident, $meter:ident $($rest:tt)*) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter [$($rest)*]
            $meter.f64_histogram(::std::stringify!($name)) }
    };

    // Entry with `name:` override
    (@build $vis:vis $name:ident, $meter:ident
        [name: $metric_name:literal $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter [$($rest)*]
            $meter.f64_histogram($metric_name) }
    };

    // `description:` keyword
    (@build $vis:vis $name:ident, $meter:ident
        [description: $v:literal $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter [$($rest)*]
            $($expr)* .with_description($v) }
    };

    // `unit:` keyword
    (@build $vis:vis $name:ident, $meter:ident
        [unit: $v:literal $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter [$($rest)*]
            $($expr)* .with_unit($v) }
    };

    // `buckets:` keyword (last keyword)
    (@build $vis:vis $name:ident, $meter:ident
        [buckets: $v:expr]
        $($expr:tt)*
    ) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter []
            $($expr)* .with_boundaries($v) }
    };
    // `buckets:` keyword (more keywords follow)
    (@build $vis:vis $name:ident, $meter:ident
        [buckets: $v:expr, $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter [$($rest)*]
            $($expr)* .with_boundaries($v) }
    };

    // Comma skimmer
    (@build $vis:vis $name:ident, $meter:ident
        [, $($rest:tt)*]
        $($expr:tt)*
    ) => {
        $crate::histogram_metric_f64! { @build $vis $name, $meter [$($rest)*]
            $($expr)* }
    };

    // Base case: no more tokens, emit histogram
    (@build $vis:vis $name:ident, $meter:ident [] $($expr:tt)*) => {
        $vis static $name: ::std::sync::LazyLock<
            ::opentelemetry::metrics::Histogram<f64>
        > = ::std::sync::LazyLock::new(|| { $($expr)* .build() });
    };
}

/// Create a [`&[KeyValue]`][::opentelemetry::KeyValue] array from key-value pairs.
#[macro_export]
macro_rules! metric_attributes {
    ($(($key:expr, $value:expr $(,)?)),+ $(,)?) => {
        &[$(::opentelemetry::KeyValue::new($key, $value)),+]
    };
}
