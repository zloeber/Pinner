use pinner_toolchain::CommandRunner;

/// HTTP GET via `curl` (process) for registry/index fetches.
///
/// Prefer `PINNER_*_RESOLVE_MAP` in unit tests; call real network only when
/// online resolve is intended (typically gated by `PINNER_NETWORK=1` in tests).
pub fn http_get(runner: &dyn CommandRunner, url: &str) -> Result<String, String> {
    let output = runner
        .run(
            "curl",
            &["-fsSL", "--max-time", "60", "--connect-timeout", "15", url],
        )
        .map_err(|err| format!("curl {url}: {err}"))?;
    if output.status != 0 {
        return Err(format!(
            "curl {url} failed (status {}): {}",
            output.status,
            output.stderr.trim()
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::http_get;
    use pinner_toolchain::{CommandOutput, CommandRunner, ToolchainError};

    struct FakeCurl {
        body: String,
    }

    impl CommandRunner for FakeCurl {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, ToolchainError> {
            assert_eq!(program, "curl");
            assert!(args.contains(&"https://example.test/index.yaml"));
            Ok(CommandOutput {
                status: 0,
                stdout: self.body.clone(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn http_get_returns_curl_stdout() {
        let runner = FakeCurl {
            body: "ok-body".into(),
        };
        let body = http_get(&runner, "https://example.test/index.yaml").unwrap();
        assert_eq!(body, "ok-body");
    }
}
