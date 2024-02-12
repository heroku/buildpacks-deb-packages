#[macro_export]
macro_rules! assert_matches {
    ($left:expr, $right:expr $(,)?) => {{
        let regex = regex::Regex::new($right).expect("should be a valid regex");
        if !regex.is_match(&$left) {
            ::std::panic!(
                r#"assertion failed: `(left matches right)`
left (unescaped):
{}

left (escaped): `{:?}`
right: `{:?}`"#,
                $left,
                $left,
                $right,
            )
        }
    }};

    ($left:expr, $right:expr, $($arg:tt)+) => {{
        let regex = regex::Regex::new($right).expect("should be a valid regex");
        if !regex.is_match(&$left) {
            ::std::panic!(
                r#"assertion failed: `(left matches right)`
left (unescaped):
{}

left (escaped): `{:?}`
right: `{:?}`: {}"#,
                $left,
                $left,
                $right,
                ::core::format_args!($($arg)+)
            )
        }
    }};
}
