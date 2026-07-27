#[cfg(unix)]
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{MAX_FRAME_BYTES, decode_response_frame, encode_request_frame};
use crate::{GatewayError, GatewayRequest, GatewayResponse, SUBSYSTEM_NAME};

const SSH_BINARY: &str = "/usr/bin/ssh";
const SSH_CONFIG: &str = "/dev/null";
const GATEWAY_CONFIG_RELATIVE_PATH: &str = ".config/lightflow";
const SSH_IDENTITY_FILE_NAME: &str = "xry_gateway_identity";
const SSH_KNOWN_HOSTS_FILE_NAME: &str = "xry_gateway_known_hosts";
const SSH_USER: &str = "lightflow-xry";
const SSH_DESTINATION: &str = "xry";
const HOST_KEY_ALIAS_OPTION: &str = "HostKeyAlias=lightflow-xry-gateway-v1";
const MAX_STDERR_BYTES: usize = 64 * 1024;
const GATEWAY_TIMEOUT: Duration = Duration::from_secs(4 * 60 * 60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
struct GatewayPaths {
    home: PathBuf,
    identity: PathBuf,
    known_hosts: PathBuf,
}

#[cfg(unix)]
enum ParentOwnership<'a> {
    Root,
    User { home: &'a Path, uid: u32 },
}

/// Invoke only the fixed XRY SSH subsystem with one framed protocol request.
///
/// This function refuses to run until its executable, identity, known-hosts
/// file, and parent directories meet the pinned deployment policy.
pub fn invoke(request: &GatewayRequest) -> Result<GatewayResponse, GatewayError> {
    let paths = trusted_gateway_paths()?;
    let request_frame = encode_request_frame(request)?;
    let mut command = Command::new(SSH_BINARY);
    command
        .args(ssh_arguments(&paths))
        .env_clear()
        .env("HOME", &paths.home)
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        GatewayError::new(format!("cannot start pinned gateway transport: {error}"))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GatewayError::new("gateway transport did not expose stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| GatewayError::new("gateway transport did not expose stderr"))?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, MAX_FRAME_BYTES + 4));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_STDERR_BYTES));

    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| GatewayError::new("gateway transport did not expose stdin"))
        .and_then(|mut stdin| {
            stdin
                .write_all(&request_frame)
                .and_then(|()| stdin.flush())
                .map_err(|error| GatewayError::new(format!("cannot send gateway request: {error}")))
        });
    if let Err(error) = write_result {
        terminate(&mut child);
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err(error);
    }

    let status = match wait_for_exit(&mut child) {
        Ok(status) => status,
        Err(error) => {
            terminate(&mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(error);
        }
    };
    let stdout = join_reader(stdout_reader, "stdout")?;
    let _stderr = join_reader(stderr_reader, "stderr")?;
    if !status.success() {
        return Err(GatewayError::new(
            "gateway subsystem exited without canonical PASS",
        ));
    }
    decode_response_frame(&stdout, request)
}

/// Static command-line arguments for the only permitted transport route.
/// No caller-controlled value is ever appended to this list.
fn ssh_arguments(paths: &GatewayPaths) -> Vec<OsString> {
    let known_hosts_option = known_hosts_option(&paths.known_hosts);
    vec![
        OsString::from("-F"),
        OsString::from(SSH_CONFIG),
        OsString::from("-o"),
        OsString::from("BatchMode=yes"),
        OsString::from("-o"),
        OsString::from("KbdInteractiveAuthentication=no"),
        OsString::from("-o"),
        OsString::from("PasswordAuthentication=no"),
        OsString::from("-o"),
        OsString::from("PreferredAuthentications=publickey"),
        OsString::from("-o"),
        OsString::from("PubkeyAuthentication=yes"),
        OsString::from("-o"),
        OsString::from("IdentitiesOnly=yes"),
        OsString::from("-i"),
        paths.identity.clone().into_os_string(),
        OsString::from("-o"),
        OsString::from("IdentityAgent=none"),
        OsString::from("-o"),
        OsString::from("AddKeysToAgent=no"),
        OsString::from("-o"),
        OsString::from("StrictHostKeyChecking=yes"),
        OsString::from("-o"),
        known_hosts_option,
        OsString::from("-o"),
        OsString::from("GlobalKnownHostsFile=/dev/null"),
        OsString::from("-o"),
        OsString::from(HOST_KEY_ALIAS_OPTION),
        OsString::from("-o"),
        OsString::from("CheckHostIP=no"),
        OsString::from("-o"),
        OsString::from("UpdateHostKeys=no"),
        OsString::from("-o"),
        OsString::from("VerifyHostKeyDNS=no"),
        OsString::from("-o"),
        OsString::from("CanonicalizeHostname=no"),
        OsString::from("-o"),
        OsString::from("ProxyCommand=none"),
        OsString::from("-o"),
        OsString::from("ProxyJump=none"),
        OsString::from("-o"),
        OsString::from("PermitLocalCommand=no"),
        OsString::from("-o"),
        OsString::from("LocalCommand=none"),
        OsString::from("-o"),
        OsString::from("RemoteCommand=none"),
        OsString::from("-o"),
        OsString::from("RequestTTY=no"),
        OsString::from("-o"),
        OsString::from("ForwardAgent=no"),
        OsString::from("-o"),
        OsString::from("ForwardX11=no"),
        OsString::from("-o"),
        OsString::from("ForwardX11Trusted=no"),
        OsString::from("-o"),
        OsString::from("ClearAllForwardings=yes"),
        OsString::from("-o"),
        OsString::from("ControlMaster=no"),
        OsString::from("-o"),
        OsString::from("ControlPath=none"),
        OsString::from("-o"),
        OsString::from("ControlPersist=no"),
        OsString::from("-o"),
        OsString::from("EscapeChar=none"),
        OsString::from("-o"),
        OsString::from("LogLevel=ERROR"),
        OsString::from("-l"),
        OsString::from(SSH_USER),
        OsString::from("-T"),
        OsString::from("-s"),
        OsString::from(SSH_DESTINATION),
        OsString::from(SUBSYSTEM_NAME),
    ]
}

fn known_hosts_option(path: &Path) -> OsString {
    let mut option = OsString::from("UserKnownHostsFile=");
    option.push(path);
    option
}

/// Verify that the fixed client-side deployment prerequisites are trusted.
pub fn trusted_transport_ready() -> Result<(), GatewayError> {
    trusted_gateway_paths().map(|_| ())
}

fn trusted_gateway_paths() -> Result<GatewayPaths, GatewayError> {
    #[cfg(not(unix))]
    {
        Err(GatewayError::new(
            "the XRY gateway transport requires a Unix host",
        ))
    }
    #[cfg(unix)]
    {
        let home = user_home_directory()?;
        let paths = trusted_user_gateway_paths_at(&home)?;
        trusted_root_parent_directories(&home)?;
        trusted_regular_file(Path::new(SSH_BINARY), 0, true, false, ParentOwnership::Root)?;
        Ok(paths)
    }
}

#[cfg(unix)]
fn user_home_directory() -> Result<PathBuf, GatewayError> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty() && path.is_absolute())
        .ok_or_else(|| GatewayError::new("the XRY gateway requires an absolute HOME directory"))?;
    Ok(home)
}

#[cfg(unix)]
fn trusted_user_gateway_paths_at(home: &Path) -> Result<GatewayPaths, GatewayError> {
    if !home.is_absolute() {
        return Err(GatewayError::new(
            "the XRY gateway requires an absolute HOME directory",
        ));
    }
    let config = home.join(GATEWAY_CONFIG_RELATIVE_PATH);
    let paths = GatewayPaths {
        home: home.to_path_buf(),
        identity: config.join(SSH_IDENTITY_FILE_NAME),
        known_hosts: config.join(SSH_KNOWN_HOSTS_FILE_NAME),
    };
    let uid = current_euid();
    trusted_regular_file(
        &paths.identity,
        uid,
        false,
        true,
        ParentOwnership::User { home, uid },
    )?;
    trusted_regular_file(
        &paths.known_hosts,
        uid,
        false,
        false,
        ParentOwnership::User { home, uid },
    )?;
    Ok(paths)
}

#[cfg(unix)]
fn current_euid() -> u32 {
    // SAFETY: `geteuid` has no preconditions and only reads the current process credentials.
    unsafe { libc::geteuid() }
}

fn wait_for_exit(child: &mut Child) -> Result<ExitStatus, GatewayError> {
    let deadline = Instant::now() + GATEWAY_TIMEOUT;
    loop {
        match child
            .try_wait()
            .map_err(|error| GatewayError::new(format!("cannot poll gateway transport: {error}")))?
        {
            Some(status) => return Ok(status),
            None if Instant::now() >= deadline => {
                return Err(GatewayError::new("gateway transport timed out"));
            }
            None => thread::sleep(POLL_INTERVAL),
        }
    }
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_bounded(mut stream: impl Read, limit: usize) -> Result<Vec<u8>, GatewayError> {
    let mut bytes = Vec::new();
    stream
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            GatewayError::new(format!("cannot read gateway transport stream: {error}"))
        })?;
    if bytes.len() > limit {
        return Err(GatewayError::new(
            "gateway transport output exceeds its limit",
        ));
    }
    Ok(bytes)
}

fn join_reader(
    reader: thread::JoinHandle<Result<Vec<u8>, GatewayError>>,
    stream_name: &str,
) -> Result<Vec<u8>, GatewayError> {
    reader
        .join()
        .map_err(|_| GatewayError::new(format!("gateway {stream_name} reader panicked")))?
}

#[cfg(unix)]
fn trusted_regular_file(
    path: &Path,
    expected_uid: u32,
    executable: bool,
    confidential: bool,
    parent_ownership: ParentOwnership<'_>,
) -> Result<(), GatewayError> {
    use std::os::unix::fs::MetadataExt;

    match parent_ownership {
        ParentOwnership::Root => trusted_root_parent_directories(path)?,
        ParentOwnership::User { home, uid } => trusted_user_parent_directories(path, home, uid)?,
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| GatewayError::new("a pinned gateway transport file is missing"))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(GatewayError::new(
            "a pinned gateway transport path is not a regular file",
        ));
    }
    if metadata.uid() != expected_uid {
        return Err(GatewayError::new(
            "a fixed gateway file has an unexpected owner",
        ));
    }
    let mode = metadata.mode();
    if mode & 0o022 != 0 {
        return Err(GatewayError::new(
            "a pinned gateway transport file is group- or world-writable",
        ));
    }
    if confidential && mode & 0o077 != 0 {
        return Err(GatewayError::new(
            "the gateway identity is readable outside its owning user",
        ));
    }
    if executable && mode & 0o100 == 0 {
        return Err(GatewayError::new(
            "the pinned SSH executable is not owner-executable",
        ));
    }
    if expected_uid != 0 {
        fs::File::open(path).map_err(|_| {
            GatewayError::new("a fixed per-user gateway configuration file is not readable")
        })?;
    }
    Ok(())
}

#[cfg(unix)]
fn trusted_root_parent_directories(path: &Path) -> Result<(), GatewayError> {
    use std::os::unix::fs::MetadataExt;

    let mut current = path.parent();
    while let Some(directory) = current {
        let metadata = fs::symlink_metadata(directory)
            .map_err(|_| GatewayError::new("a pinned gateway transport directory is missing"))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(GatewayError::new(
                "a pinned gateway transport parent is not a directory",
            ));
        }
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(GatewayError::new(
                "a pinned gateway transport directory is not trusted",
            ));
        }
        current = directory.parent();
    }
    Ok(())
}

#[cfg(unix)]
fn trusted_user_parent_directories(path: &Path, home: &Path, uid: u32) -> Result<(), GatewayError> {
    use std::os::unix::fs::MetadataExt;

    if !path.starts_with(home) {
        return Err(GatewayError::new(
            "a fixed per-user gateway path escapes HOME",
        ));
    }
    let mut current = path.parent();
    loop {
        let directory = current.ok_or_else(|| {
            GatewayError::new("a fixed per-user gateway configuration directory is missing")
        })?;
        let metadata = fs::symlink_metadata(directory).map_err(|_| {
            GatewayError::new("a fixed per-user gateway configuration directory is missing")
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(GatewayError::new(
                "a fixed per-user gateway parent is not a directory",
            ));
        }
        if metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
            return Err(GatewayError::new(
                "a fixed per-user gateway directory is not trusted",
            ));
        }
        if directory == home {
            break;
        }
        current = directory.parent();
    }
    Ok(())
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
