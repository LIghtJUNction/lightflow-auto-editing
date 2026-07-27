use super::*;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn pair_exists(arguments: &[OsString], left: &str, right: &str) -> bool {
    arguments
        .windows(2)
        .any(|pair| pair[0] == OsStr::new(left) && pair[1] == OsStr::new(right))
}

fn triple_exists(arguments: &[OsString], first: &str, second: &str, third: &str) -> bool {
    arguments.windows(3).any(|triple| {
        triple[0] == OsStr::new(first)
            && triple[1] == OsStr::new(second)
            && triple[2] == OsStr::new(third)
    })
}

fn fixture_paths() -> GatewayPaths {
    let home = PathBuf::from("/home/lightflow");
    let config = home.join(GATEWAY_CONFIG_RELATIVE_PATH);
    GatewayPaths {
        home,
        identity: config.join(SSH_IDENTITY_FILE_NAME),
        known_hosts: config.join(SSH_KNOWN_HOSTS_FILE_NAME),
    }
}

#[test]
fn ssh_route_is_fixed_and_is_a_subsystem() {
    let arguments = ssh_arguments(&fixture_paths());
    assert_eq!(
        arguments.first().map(OsString::as_os_str),
        Some(OsStr::new("-F"))
    );
    assert!(pair_exists(&arguments, "-F", "/dev/null"));
    assert!(pair_exists(&arguments, "-o", "ProxyCommand=none"));
    assert!(pair_exists(&arguments, "-o", "StrictHostKeyChecking=yes"));
    assert!(pair_exists(
        &arguments,
        "-o",
        "UserKnownHostsFile=/home/lightflow/.config/lightflow/xry_gateway_known_hosts"
    ));
    assert!(triple_exists(
        &arguments,
        "-s",
        SSH_DESTINATION,
        SUBSYSTEM_NAME
    ));
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.to_string_lossy().contains("/srv/"))
    );
}

#[cfg(unix)]
#[test]
fn fixed_user_config_requires_private_files_and_trusted_ancestry() {
    let home = tempfile::Builder::new()
        .prefix("lightflow-xry-gateway-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .expect("temporary home");
    let config = home.path().join(GATEWAY_CONFIG_RELATIVE_PATH);
    fs::create_dir_all(&config).expect("gateway config directory");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700))
        .expect("private gateway config directory");
    let identity = config.join(SSH_IDENTITY_FILE_NAME);
    let known_hosts = config.join(SSH_KNOWN_HOSTS_FILE_NAME);
    fs::write(&identity, b"test identity").expect("identity");
    fs::write(&known_hosts, b"xry ssh-ed25519 test-key").expect("known hosts");
    fs::set_permissions(&identity, fs::Permissions::from_mode(0o600)).expect("private identity");
    fs::set_permissions(&known_hosts, fs::Permissions::from_mode(0o600))
        .expect("private known hosts");

    let paths = trusted_user_gateway_paths_at(home.path()).expect("trusted user config");
    assert_eq!(paths.identity, identity);
    assert_eq!(paths.known_hosts, known_hosts);

    let uid = current_euid();
    let different_uid = if uid == u32::MAX { 0 } else { uid + 1 };
    let error = trusted_regular_file(
        &identity,
        different_uid,
        false,
        true,
        ParentOwnership::User {
            home: home.path(),
            uid,
        },
    )
    .expect_err("identity must belong to the invoking user");
    assert!(error.to_string().contains("unexpected owner"));

    fs::set_permissions(&identity, fs::Permissions::from_mode(0o640))
        .expect("group-readable identity");
    let error = trusted_user_gateway_paths_at(home.path()).expect_err("identity must stay private");
    assert!(
        error
            .to_string()
            .contains("identity is readable outside its owning user")
    );

    fs::set_permissions(&identity, fs::Permissions::from_mode(0o600))
        .expect("restore private identity");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o770))
        .expect("group-writable gateway config directory");
    let error =
        trusted_user_gateway_paths_at(home.path()).expect_err("config directory must be trusted");
    assert!(error.to_string().contains("directory is not trusted"));

    fs::set_permissions(&config, fs::Permissions::from_mode(0o700))
        .expect("restore private gateway config directory");
    fs::remove_file(&identity).expect("remove identity fixture");
    std::os::unix::fs::symlink(&known_hosts, &identity).expect("symlink identity fixture");
    let error =
        trusted_user_gateway_paths_at(home.path()).expect_err("identity must not be a symlink");
    assert!(error.to_string().contains("not a regular file"));
}
