use super::*;

fn policy() -> Policy {
	Policy {
		program: PathBuf::from("/opt/bun/bin/bun"),
		read: vec![PathBuf::from("/opt/truapi/runner"), PathBuf::from("/state/workers/bafy")],
		scratch: PathBuf::from("/tmp/worker-scratch"),
		socket: PathBuf::from("/tmp/truapi-host-x/frames.sock"),
	}
}

#[test]
fn a_policy_survives_its_arguments() {
	let arguments = policy().arguments([OsString::from("run"), OsString::from("--flag")]);
	let (parsed, rest) = Policy::parse(arguments).unwrap();
	assert_eq!(parsed, policy());
	assert_eq!(rest, [OsString::from("run"), OsString::from("--flag")]);
}

#[test]
fn a_policy_needs_absolute_paths_a_scratch_directory_and_a_socket() {
	let parse = |arguments: &[&str]| Policy::parse(arguments.iter().map(OsString::from));
	assert!(
		parse(&["--read", "relative", "--scratch", "/s", "--socket", "/k", "--", "/bun"]).is_err()
	);
	assert!(parse(&["--scratch", "/s", "--socket", "/k", "--", "bun"]).is_err());
	assert!(parse(&["--socket", "/k", "--", "/bun"]).is_err());
	assert!(parse(&["--scratch", "/s", "--", "/bun"]).is_err());
	assert!(
		parse(&["--scratch", "/s", "--socket", "/k", "--exec", "/bin/sh", "--", "/bun"]).is_err()
	);
	assert!(parse(&["--scratch", "/s", "--socket", "/k"]).is_err());
}

#[test]
fn the_macos_profile_denies_by_default_and_grants_the_policy() {
	let profile = policy().macos_profile();
	let lines: Vec<&str> = profile.lines().filter(|line| !line.starts_with(';')).collect();
	assert_eq!(
		lines,
		[
			"(version 1)",
			"(deny default)",
			"(import \"system.sb\")",
			"(allow process-exec (literal \"/opt/bun/bin/bun\"))",
			"(allow file-read* (literal \"/opt/bun/bin/bun\"))",
			"(allow file-read* (subpath \"/opt/truapi/runner\") (subpath \"/state/workers/bafy\"))",
			"(allow file-read-metadata (path-ancestors \"/opt/truapi/runner\") \
			 (path-ancestors \"/state/workers/bafy\") (path-ancestors \"/tmp/worker-scratch\"))",
			"(allow file-read* file-write* (subpath \"/tmp/worker-scratch\"))",
			"(allow network-outbound (literal \"/tmp/truapi-host-x/frames.sock\"))",
			"(allow mach-lookup (global-name \"com.apple.dnssd.service\"))",
			"(allow network-outbound (literal \"/private/var/run/mDNSResponder\"))",
			"(allow network-outbound (remote tcp \"*:80\") (remote tcp \"*:443\"))",
		]
	);
	// Nothing grants executing another program, writing elsewhere, or
	// connecting to any other socket than the frame socket and the resolver's.
	assert_eq!(profile.matches("process-exec").count(), 1);
	assert_eq!(profile.matches("file-write").count(), 1);
	assert_eq!(profile.matches("network-outbound").count(), 3);
	assert!(!profile.contains("process-fork"));
}

#[test]
fn the_macos_profile_quotes_paths() {
	let mut policy = policy();
	policy.scratch = PathBuf::from("/tmp/a \"b\\c");
	assert!(
		policy
			.macos_profile()
			.contains("(allow file-read* file-write* (subpath \"/tmp/a \\\"b\\\\c\"))")
	);
}

#[test]
fn the_landlock_rules_grant_the_policy_and_the_system_libraries_bun_loads() {
	let rules = LandlockRules::of(&policy());
	assert!(rules.read.ends_with(&policy().read));
	assert!(rules.read.contains(&PathBuf::from("/usr/lib")));
	assert!(rules.read.contains(&PathBuf::from("/etc/resolv.conf")));
	assert!(
		!rules
			.read
			.iter()
			.any(|path| path.starts_with("/proc") || path.starts_with("/home"))
	);
	assert_eq!(rules.execute[0], PathBuf::from("/opt/bun/bin/bun"));
	assert!(rules.execute[1..].iter().all(|path| {
		path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("ld-"))
	}));
	assert_eq!(rules.write, [PathBuf::from("/tmp/worker-scratch"), PathBuf::from("/dev/null")]);
	assert_eq!(rules.connect, [PathBuf::from("/tmp/truapi-host-x/frames.sock")]);
}
