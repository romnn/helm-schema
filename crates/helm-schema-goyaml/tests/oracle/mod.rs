//! The sole JSONL oracle process owner used by integration tests.

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::Value;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub(super) struct Oracle {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Oracle {
    pub(super) fn start() -> eyre::Result<Self> {
        let program = std::env::var_os("HELM_SCHEMA_GOYAMLSWEEP").map_or_else(
            || {
                test_util::scratch::target_dir()
                    .join(format!("goyamlsweep{}", std::env::consts::EXE_SUFFIX))
            },
            PathBuf::from,
        );
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().ok_or_eyre("no oracle stdin")?;
        let output = BufReader::new(child.stdout.take().ok_or_eyre("no oracle stdout")?);
        Ok(Self {
            child,
            input: Some(input),
            output,
        })
    }

    pub(super) fn request(&mut self, request: &Value) -> eyre::Result<Value> {
        let input = self.input.as_mut().ok_or_eyre("oracle closed")?;
        serde_json::to_writer(&mut *input, request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            eyre::bail!("oracle exited without a response");
        }
        Ok(serde_json::from_str(&line)?)
    }

    pub(super) fn finish(mut self) -> eyre::Result<()> {
        self.input.take();
        let status = self.child.wait()?;
        if !status.success() {
            eyre::bail!("oracle exited {status}");
        }
        Ok(())
    }
}

impl Drop for Oracle {
    fn drop(&mut self) {
        self.input.take();
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            if let Err(err) = self.child.kill() {
                eprintln!("failed to stop oracle: {err}");
            }
            if let Err(err) = self.child.wait() {
                eprintln!("failed to reap oracle: {err}");
            }
        }
    }
}
