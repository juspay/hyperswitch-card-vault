/// Returns the service (binary) name.
#[macro_export]
macro_rules! service_name {
    () => {
        env!("CARGO_BIN_NAME")
    };
}

/// Returns the full version string with git info.
#[cfg(feature = "vergen")]
#[macro_export]
macro_rules! version {
    () => {
        concat!(
            build_info::git_describe!(),
            "-",
            build_info::git_sha!(),
            "-",
            build_info::git_commit_timestamp!()
        )
    };
}
