use crate::{Error, Result};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use wait_timeout::ChildExt;

// Drain pipes even after the output budget is reached, so children cannot deadlock.
pub(crate) fn run(
    cmd: &mut Command,
    input: Option<Vec<u8>>,
    timeout: u64,
    limit: usize,
) -> Result<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take();
    let writer = thread::spawn(move || {
        if let (Some(mut pipe), Some(data)) = (stdin, input) {
            let _ = pipe.write_all(&data);
        }
    });
    fn drain(
        mut pipe: impl Read + Send + 'static,
        limit: usize,
    ) -> thread::JoinHandle<(Vec<u8>, bool)> {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut buf = [0; 8192];
            let mut overflow = false;
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let keep = n.min(limit.saturating_sub(bytes.len()));
                bytes.extend_from_slice(&buf[..keep]);
                overflow |= keep < n;
            }
            (bytes, overflow)
        })
    }
    let out = drain(child.stdout.take().unwrap(), limit);
    let err = drain(child.stderr.take().unwrap(), 8192);
    let status = child.wait_timeout(Duration::from_secs(timeout))?;
    #[cfg(unix)]
    {
        // Kill descendants as well, including validators that leave background pipe holders.
        // SAFETY: the child was placed in its own process group above.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        // Descendants may retain pipes. Dropping reader handles avoids an unbounded join.
        return Err(Error::Process("timeout".into()));
    }
    let _ = writer.join();
    let (bytes, overflow) = out
        .join()
        .map_err(|_| Error::Process("output reader".into()))?;
    let _ = err.join(); // Never expose process stderr: it may contain credentials or untrusted output.
    if !status.unwrap().success() {
        return Err(Error::Process("command exited unsuccessfully".into()));
    }
    if overflow {
        return Err(Error::Limit("subprocess output".into()));
    }
    Ok(bytes)
}
