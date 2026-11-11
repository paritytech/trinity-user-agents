//! Network-scoped signing-host session directories and current selection.

use std::{
	fs,
	fs::OpenOptions,
	path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use truapi::host_logic::dotns_gateway::is_lite_label;

use crate::accounts;

pub const DEFAULT_SESSION_NAME: &str = "default";
const CURRENT_SESSION_FILE: &str = "current-session";
const SESSION_INFO_FILE: &str = "session.json";
const PAIRED_HOSTS_FILE: &str = "paired-hosts.json";
const PAIRED_HOSTS_LOCK_FILE: &str = "paired-hosts.json.lock";
const PAIRED_HOST_VERSION: u32 = 1;
const PAIRED_HOST_STORE_VERSION: u32 = 1;

/// Which session a base path restores when no name is selected.
///
/// The pointer file names it. It can be lost or left naming a session that no
/// longer exists, so the provisioned session directories are consulted in that
/// case rather than falling straight back to the bootstrap profile, which would
/// provision a second identity beside the one already on disk.
#[derive(Clone, Debug, PartialEq)]
pub enum CurrentSession {
	/// The pointer file named a session that exists.
	Pointed(String),
	/// The pointer was missing or stale and exactly one provisioned session
	/// was adopted in its place.
	Recovered(String),
	/// The pointer was missing or stale and more than one provisioned session
	/// could have been meant. Picking one would bind an identity by accident.
	Ambiguous {
		/// Every provisioned session the pointer could have named.
		candidates: Vec<String>,
	},
	/// No provisioned session exists yet.
	Fresh,
}

/// Safe display metadata retained from a paired host's proposal.
#[derive(Clone, Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct PairedHostMetadata {
	/// Human-readable host name.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub host_name: Option<String>,
	/// Host software version.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub host_version: Option<String>,
	/// Host icon URL.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub host_icon: Option<String>,
	/// Platform kind, such as a browser or operating system name.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub platform_type: Option<String>,
	/// Platform version.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub platform_version: Option<String>,
}

/// Versioned public data for a host paired with one managed session.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PairedHost {
	version: u32,
	statement_account_id: [u8; 32],
	encryption_public_key: [u8; 32],
	#[serde(flatten)]
	metadata: PairedHostMetadata,
}

impl PairedHost {
	/// Create the current persisted representation of a paired host.
	pub fn new(
		statement_account_id: [u8; 32],
		encryption_public_key: [u8; 32],
		metadata: PairedHostMetadata,
	) -> Self {
		Self { version: PAIRED_HOST_VERSION, statement_account_id, encryption_public_key, metadata }
	}

	/// Return the statement account ID that uniquely identifies this host.
	pub fn statement_account_id(&self) -> [u8; 32] {
		self.statement_account_id
	}

	/// Return the host's public encryption key.
	pub fn encryption_public_key(&self) -> [u8; 32] {
		self.encryption_public_key
	}

	/// Return safe display metadata retained from the pairing proposal.
	pub fn metadata(&self) -> &PairedHostMetadata {
		&self.metadata
	}
}

#[derive(Serialize, Deserialize)]
struct PairedHostStore {
	version: u32,
	#[serde(default)]
	paired_hosts: Vec<PairedHost>,
}

impl Default for PairedHostStore {
	fn default() -> Self {
		Self { version: PAIRED_HOST_STORE_VERSION, paired_hosts: Vec::new() }
	}
}

struct PairedHostStoreLock {
	file: fs::File,
}

impl PairedHostStoreLock {
	fn acquire(session_path: &Path) -> Result<Self> {
		fs::create_dir_all(session_path)
			.with_context(|| format!("create session {}", session_path.display()))?;
		let path = session_path.join(PAIRED_HOSTS_LOCK_FILE);
		let file = OpenOptions::new()
			.create(true)
			.truncate(false)
			.read(true)
			.write(true)
			.open(&path)
			.with_context(|| format!("open paired host lock {}", path.display()))?;
		file.lock_exclusive()
			.with_context(|| format!("lock paired hosts {}", path.display()))?;
		Ok(Self { file })
	}
}

impl Drop for PairedHostStoreLock {
	fn drop(&mut self) {
		let _ = self.file.unlock();
	}
}

/// Persistent signing-host session data selected for permanent removal.
#[derive(Clone, PartialEq, Debug)]
pub enum SessionClearTarget {
	/// Clear one durable session name shown by `/session --list`.
	Named(String),
	/// Clear every managed signing-host session for the active network.
	All,
}

#[derive(Serialize, Deserialize)]
struct SessionInfo {
	version: u32,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	user_id: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	account_name: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	last_script: Option<String>,
	/// The provisional name this session was promoted from, when it differs
	/// from the durable one.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	created_as: Option<String>,
}

impl Default for SessionInfo {
	fn default() -> Self {
		Self { version: 1, user_id: None, account_name: None, last_script: None, created_as: None }
	}
}

/// Filesystem locations owned by one managed signing-host session.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionProfile {
	pub name: String,
	/// Core/session state directory shown by `/session`.
	pub path: PathBuf,
	/// Product-local KV directory for this named session.
	pub product_storage_dir: PathBuf,
	/// Directory containing this session's `accounts.json`.
	pub account_base_path: PathBuf,
}

impl SessionProfile {
	/// Whether this session already holds a signer.
	///
	/// A session directory can exist without one, because inspecting a name
	/// creates it. Only an account store means an identity was provisioned.
	pub fn is_provisioned(&self) -> bool {
		accounts::has_account_store(&self.account_base_path)
	}
}

/// A promoted profile whose original storage is restored unless activation commits.
pub struct SessionPromotion {
	profile: SessionProfile,
	moved_paths: Vec<(PathBuf, PathBuf)>,
	created_target: bool,
	original_metadata: Option<Vec<u8>>,
	committed: bool,
}

impl SessionPromotion {
	/// Borrow the prepared locations while retaining rollback ownership.
	pub fn profile(&self) -> &SessionProfile {
		&self.profile
	}

	/// Retain the promoted storage after successful activation and selection.
	pub fn commit(mut self) -> SessionProfile {
		self.committed = true;
		self.profile.clone()
	}

	fn move_path(&mut self, source: &Path, target: PathBuf) -> Result<()> {
		fs::rename(source, &target)
			.with_context(|| format!("move {} to {}", source.display(), target.display()))?;
		self.moved_paths.push((source.to_path_buf(), target));
		Ok(())
	}
}

impl Drop for SessionPromotion {
	fn drop(&mut self) {
		if self.committed || self.moved_paths.is_empty() && !self.created_target {
			return;
		}
		let metadata = self.profile.path.join(SESSION_INFO_FILE);
		let restoration = match &self.original_metadata {
			Some(contents) => fs::write(&metadata, contents).map_err(anyhow::Error::from),
			None => remove_file_if_exists(&metadata),
		};
		let mut restored = match restoration {
			Ok(()) => true,
			Err(error) => {
				tracing::error!(%error, path = %metadata.display(), "failed to restore session metadata");
				false
			},
		};
		for (source, target) in self.moved_paths.iter().rev() {
			if source.exists() {
				tracing::error!(path = %source.display(), "session rollback destination already exists");
				restored = false;
			} else if let Err(error) = fs::rename(target, source) {
				tracing::error!(%error, path = %source.display(), "failed to restore session storage");
				restored = false;
			}
		}
		if self.created_target &&
			restored && let Err(error) = fs::remove_dir_all(&self.profile.path)
		{
			tracing::error!(%error, path = %self.profile.path.display(), "failed to remove uncommitted session profile");
		}
	}
}

/// Persistent session catalog for one network.
#[derive(Clone)]
pub struct SessionCatalog {
	base_path: PathBuf,
	network_id: String,
	network_path: PathBuf,
	role_path: PathBuf,
}

impl SessionCatalog {
	pub fn new(base_path: PathBuf, network_id: &str) -> Result<Self> {
		let base_path = absolute_path(base_path)?;
		let network_path = base_path.join(network_id);
		let role_path = network_path.join("signing-host");
		fs::create_dir_all(&role_path)
			.with_context(|| format!("create session root {}", role_path.display()))?;
		Ok(Self { base_path, network_id: network_id.to_string(), network_path, role_path })
	}

	pub fn profile(&self, name: &str) -> Result<SessionProfile> {
		validate_name(name)?;
		if name == DEFAULT_SESSION_NAME {
			return Ok(SessionProfile {
				name: name.to_string(),
				path: self.role_path.clone(),
				product_storage_dir: self.role_path.join("storage").join(name),
				// Preserve the pre-session account store for compatibility.
				account_base_path: self.base_path.clone(),
			});
		}
		let path = self.identity_path(name);
		Ok(SessionProfile {
			name: name.to_string(),
			path: path.clone(),
			product_storage_dir: path.join("storage"),
			account_base_path: path,
		})
	}

	pub fn ensure_profile(&self, name: &str) -> Result<SessionProfile> {
		let profile = self.profile(name)?;
		fs::create_dir_all(&profile.path)
			.with_context(|| format!("create session {}", profile.path.display()))?;
		Ok(profile)
	}

	pub fn exists(&self, name: &str) -> bool {
		self.profile(name)
			.is_ok_and(|profile| name == DEFAULT_SESSION_NAME || profile.path.is_dir())
	}

	/// Resolve which session this base path restores, and how confidently.
	///
	/// A session counts as provisioned only once it holds an account store, so
	/// a bare directory left by a name that was only ever inspected is not
	/// mistaken for an identity.
	pub fn current_session(&self) -> Result<CurrentSession> {
		if let Ok(name) = fs::read_to_string(self.role_path.join(CURRENT_SESSION_FILE)) {
			let name = name.trim();
			if self.exists(name) {
				return Ok(CurrentSession::Pointed(name.to_string()));
			}
		}
		let mut provisioned: Vec<String> = self
			.list()?
			.into_iter()
			.filter(|name| self.profile(name).is_ok_and(|profile| profile.is_provisioned()))
			.collect();
		Ok(match provisioned.len() {
			0 => CurrentSession::Fresh,
			1 => CurrentSession::Recovered(provisioned.remove(0)),
			_ => CurrentSession::Ambiguous { candidates: provisioned },
		})
	}

	/// Resolve an exact username, or the newest account matching a base or original alias.
	pub fn resolve_session_name(&self, name: &str) -> Result<String> {
		let numbered = username_base(name).is_some();
		if self.exists(name) {
			let info = read_session_info(&self.identity_path(name))?;
			if numbered || (info.user_id.is_none() && info.account_name.is_some()) {
				return Ok(name.to_string());
			}
		}
		let prefix = (!numbered).then(|| lite_username_prefix(name)).flatten();
		let mut candidates = Vec::new();
		let mut aliases = Vec::new();
		for candidate in self.list()? {
			let profile = self.profile(&candidate)?;
			let info = read_session_info(&profile.path)?;
			if (profile.is_provisioned() || info.account_name.is_some()) &&
				(candidate == name ||
					prefix
						.as_deref()
						.is_some_and(|prefix| username_base(&candidate) == Some(prefix)))
			{
				candidates.push((profile, info));
			} else if info.created_as.as_deref() == Some(name) {
				aliases.push((profile, info));
			}
		}
		if candidates.is_empty() {
			candidates = aliases;
		}
		let mut latest_created_at = None;
		let mut latest = Vec::new();
		for (profile, info) in candidates {
			let created_at = accounts::AccountStore::load(&profile.account_base_path)?
				.created_at(&self.network_id, info.account_name.as_deref())?;
			if created_at > latest_created_at {
				latest_created_at = created_at;
				latest.clear();
			}
			if created_at == latest_created_at {
				latest.push(profile.name);
			}
		}
		match latest.as_slice() {
			[] if numbered => {
				bail!("session {name:?} was not found; use /session --list to see saved sessions")
			},
			[] => Ok(name.to_string()),
			[candidate] => Ok(candidate.clone()),
			_ => {
				bail!(
					"sessions {} have the same creation time; select an exact session name",
					latest.join(", ")
				)
			},
		}
	}

	pub fn set_current(&self, name: &str) -> Result<()> {
		let profile = self.ensure_profile(name)?;
		let path = self.role_path.join(CURRENT_SESSION_FILE);
		let temporary = self
			.role_path
			.join(format!(".{CURRENT_SESSION_FILE}.{}.tmp", std::process::id()));
		fs::write(&temporary, format!("{}\n", profile.name))
			.with_context(|| format!("write current session {}", temporary.display()))?;
		fs::rename(&temporary, &path)
			.with_context(|| format!("persist current session {}", path.display()))?;
		Ok(())
	}

	pub fn list(&self) -> Result<Vec<String>> {
		let mut names = Vec::new();
		for entry in fs::read_dir(&self.network_path)
			.with_context(|| format!("list host profiles {}", self.network_path.display()))?
			.filter_map(std::result::Result::ok)
		{
			if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
				continue;
			}
			let Some(filename) = entry.file_name().to_str().map(ToOwned::to_owned) else {
				continue;
			};
			let Some(name) = filename.strip_suffix("_signing_host") else {
				continue;
			};
			if validate_name(name).is_ok() && name != DEFAULT_SESSION_NAME {
				names.push(name.to_string());
			}
		}
		names.sort();
		Ok(names)
	}

	/// Return the network whose signing-host sessions this catalog owns.
	pub fn network_id(&self) -> &str {
		&self.network_id
	}

	/// Check that a clear target currently resolves to managed session data.
	pub fn validate_clear_target(&self, target: &SessionClearTarget) -> Result<()> {
		let SessionClearTarget::Named(name) = target else {
			return Ok(());
		};
		validate_selectable_name(name)?;
		let name = &self.resolve_session_name(name)?;
		if self.list()?.iter().any(|existing| existing == name) {
			Ok(())
		} else {
			anyhow::bail!("session `{name}` does not exist; use /session --list")
		}
	}

	/// Permanently remove the selected local signing-host session data.
	///
	/// Callers must stop an active runtime before clearing its profile. The
	/// catalog only removes paths it derives from validated, listed names.
	pub fn clear(&self, target: &SessionClearTarget) -> Result<Vec<String>> {
		// Resolve once here, so a name promoted away clears the session it
		// created just as it selects it everywhere else.
		let resolved;
		let target = match target {
			SessionClearTarget::Named(name) => {
				resolved = SessionClearTarget::Named(self.resolve_session_name(name)?);
				&resolved
			},
			SessionClearTarget::All => target,
		};
		self.validate_clear_target(target)?;
		match target {
			SessionClearTarget::Named(name) => {
				let was_current = matches!(
					self.current_session()?,
					CurrentSession::Pointed(current) if current == *name
				);
				self.remove_named_data(name)?;
				if was_current {
					remove_file_if_exists(&self.role_path.join(CURRENT_SESSION_FILE))?;
				}
				accounts::remove_managed_accounts(&self.base_path, &self.network_id, Some(name))?;
				Ok(vec![name.clone()])
			},
			SessionClearTarget::All => {
				let names = self.list()?;
				for name in &names {
					self.remove_named_data(name)?;
				}
				remove_dir_if_exists(&self.role_path)?;
				accounts::remove_managed_accounts(&self.base_path, &self.network_id, None)?;
				Ok(names)
			},
		}
	}

	fn remove_named_data(&self, name: &str) -> Result<()> {
		let profile = self.profile(name)?;
		if !profile.path.starts_with(&self.network_path) {
			anyhow::bail!(
				"refusing to clear session outside network root {}",
				self.network_path.display()
			);
		}
		remove_dir_if_exists(&profile.path)
	}

	/// Move a provisional session into the user-owned host root.
	///
	/// The public session name is the Lite username. The suffix is only a
	/// filesystem discriminator so pairing and signing state cannot collide.
	#[cfg(test)]
	pub fn promote_to_user(
		&self,
		profile: &SessionProfile,
		user_id: &str,
	) -> Result<SessionProfile> {
		Ok(self.prepare_promotion(profile, user_id)?.commit())
	}

	/// Prepare a promotion that restores moved storage if activation fails or is cancelled.
	pub fn prepare_promotion(
		&self,
		profile: &SessionProfile,
		user_id: &str,
	) -> Result<SessionPromotion> {
		validate_name(user_id)?;
		let target_path = self.identity_path(user_id);
		let mut promotion = SessionPromotion {
			profile: SessionProfile {
				name: user_id.to_string(),
				path: target_path.clone(),
				product_storage_dir: target_path.join("storage"),
				account_base_path: target_path.clone(),
			},
			moved_paths: Vec::new(),
			created_target: false,
			original_metadata: None,
			committed: false,
		};
		if profile.path != target_path && !target_path.exists() {
			let metadata = profile.path.join(SESSION_INFO_FILE);
			promotion.original_metadata = match fs::read(&metadata) {
				Ok(contents) => Some(contents),
				Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
				Err(error) => {
					return Err(error)
						.with_context(|| format!("read session metadata {}", metadata.display()));
				},
			};
			if profile.path == self.role_path {
				fs::create_dir(&target_path)
					.with_context(|| format!("create user host {}", target_path.display()))?;
				promotion.created_target = true;
				migrate_default_profile(profile, &target_path, &mut promotion)?;
			} else {
				promotion.move_path(&profile.path, target_path.clone())?;
			}
		}
		let promoted = &promotion.profile;
		fs::create_dir_all(&promoted.path)
			.with_context(|| format!("create user host {}", promoted.path.display()))?;
		let mut info = read_session_info(&promoted.path)?;
		info.user_id = Some(user_id.to_string());
		if profile.name != user_id && profile.name != DEFAULT_SESSION_NAME {
			info.created_as = Some(profile.name.clone());
		}
		write_session_info(&promoted.path, &info)?;
		Ok(promotion)
	}

	pub fn cached_user_id(&self, profile: &SessionProfile) -> Result<Option<String>> {
		Ok(read_session_info(&profile.path)?.user_id.filter(|user_id| !user_id.is_empty()))
	}

	pub fn store_user_id(&self, profile: &SessionProfile, user_id: &str) -> Result<()> {
		if user_id.is_empty() {
			return Ok(());
		}
		let mut info = read_session_info(&profile.path)?;
		info.user_id = Some(user_id.to_string());
		write_session_info(&profile.path, &info)
	}

	/// Return the exact local account record bound to this durable session.
	pub fn cached_account_name(&self, profile: &SessionProfile) -> Result<Option<String>> {
		Ok(read_session_info(&profile.path)?.account_name.filter(|name| !name.is_empty()))
	}

	/// Persist the username and account record together so restart cannot fall
	/// back to an unrelated auto-managed account.
	pub fn store_signer_binding(
		&self,
		profile: &SessionProfile,
		user_id: &str,
		account_name: &str,
	) -> Result<()> {
		if user_id.is_empty() || account_name.is_empty() {
			return Ok(());
		}
		let mut info = read_session_info(&profile.path)?;
		info.user_id = Some(user_id.to_string());
		info.account_name = Some(account_name.to_string());
		write_session_info(&profile.path, &info)
	}

	/// Bind a username-less imported signer to its durable session.
	pub fn store_account_binding(
		&self,
		profile: &SessionProfile,
		account_name: &str,
	) -> Result<()> {
		if account_name.is_empty() {
			return Ok(());
		}
		let mut info = read_session_info(&profile.path)?;
		info.account_name = Some(account_name.to_string());
		write_session_info(&profile.path, &info)
	}

	/// Return all paired hosts in stable statement-account order.
	pub fn paired_hosts(&self, profile: &SessionProfile) -> Result<Vec<PairedHost>> {
		let mut paired_hosts = read_paired_host_store(&profile.path)?.paired_hosts;
		paired_hosts.sort_by_key(PairedHost::statement_account_id);
		Ok(paired_hosts)
	}

	/// Insert or replace a paired host using its statement account ID.
	pub fn store_paired_host(
		&self,
		profile: &SessionProfile,
		paired_host: PairedHost,
	) -> Result<()> {
		let _lock = PairedHostStoreLock::acquire(&profile.path)?;
		let mut store = read_paired_host_store(&profile.path)?;
		if let Some(existing) = store
			.paired_hosts
			.iter_mut()
			.find(|existing| existing.statement_account_id() == paired_host.statement_account_id())
		{
			if existing == &paired_host {
				return Ok(());
			}
			*existing = paired_host;
		} else {
			store.paired_hosts.push(paired_host);
		}
		store.paired_hosts.sort_by_key(PairedHost::statement_account_id);
		write_paired_host_store(&profile.path, &store)
	}

	/// Remove exactly one paired host selected by statement account ID.
	pub fn remove_paired_host(
		&self,
		profile: &SessionProfile,
		statement_account_id: &[u8; 32],
	) -> Result<bool> {
		let _lock = PairedHostStoreLock::acquire(&profile.path)?;
		let mut store = read_paired_host_store(&profile.path)?;
		let original_len = store.paired_hosts.len();
		store
			.paired_hosts
			.retain(|paired_host| paired_host.statement_account_id() != *statement_account_id);
		if store.paired_hosts.len() == original_len {
			return Ok(false);
		}
		write_paired_host_store(&profile.path, &store)?;
		Ok(true)
	}

	/// Return the last script used in this session, if it still exists.
	pub fn last_script(&self, profile: &SessionProfile) -> Result<Option<PathBuf>> {
		session_last_script(&profile.path)
	}

	fn identity_path(&self, user_id: &str) -> PathBuf {
		self.network_path.join(format!("{user_id}_signing_host"))
	}
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
	match fs::remove_dir_all(path) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(error).with_context(|| format!("clear session data {}", path.display())),
	}
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
	match fs::remove_file(path) {
		Ok(()) => Ok(()),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(error) => {
			Err(error).with_context(|| format!("clear session pointer {}", path.display()))
		},
	}
}

fn migrate_default_profile(
	profile: &SessionProfile,
	target_path: &Path,
	promotion: &mut SessionPromotion,
) -> Result<()> {
	for name in ["core-storage.json", SESSION_INFO_FILE, PAIRED_HOSTS_FILE] {
		let source = profile.path.join(name);
		if source.is_file() {
			promotion.move_path(&source, target_path.join(name))?;
		}
	}
	let scripts = profile.path.join("scripts");
	if scripts.is_dir() {
		promotion.move_path(&scripts, target_path.join("scripts"))?;
	}
	let chat_files = profile.path.join(crate::chat_files::DIRECTORY);
	if chat_files.is_dir() {
		promotion.move_path(&chat_files, target_path.join(crate::chat_files::DIRECTORY))?;
	}
	if profile.product_storage_dir.is_dir() {
		promotion.move_path(&profile.product_storage_dir, target_path.join("storage"))?;
	}
	let account_store = profile.account_base_path.join("accounts.json");
	if account_store.is_file() {
		fs::copy(&account_store, target_path.join("accounts.json"))
			.with_context(|| format!("copy {}", account_store.display()))?;
	}
	Ok(())
}

/// Return the last script recorded in a host/session state directory.
pub fn session_last_script(session_path: &Path) -> Result<Option<PathBuf>> {
	let info = read_session_info(session_path)?;
	let Some(filename) = info.last_script else {
		return Ok(None);
	};
	let configured = Path::new(&filename);
	if configured.is_absolute() {
		return crate::script_project::remembered_script(configured);
	}

	let relative = configured;
	let mut components = relative.components();
	let Some(Component::Normal(filename)) = components.next() else {
		anyhow::bail!("session last script is not a portable filename");
	};
	if components.next().is_some() {
		anyhow::bail!("session last script must stay inside its scripts directory");
	}
	let script = session_path.join("scripts").join(filename);
	Ok(script.is_file().then_some(script))
}

/// Record a script in a host/session state directory.
///
/// Scratch scripts are stored by filename so existing session directories stay
/// portable. Explicit scripts outside that directory are stored as absolute
/// paths so a later bare `/script` reopens the same file.
pub fn store_session_last_script(session_path: &Path, script: &Path) -> Result<()> {
	let script = absolute_path(script.to_path_buf())?;
	let scripts = session_path.join("scripts");
	let stored_path = if script.parent() == Some(scripts.as_path()) {
		script
			.file_name()
			.and_then(|filename| filename.to_str())
			.context("scratch script filename is not valid UTF-8")?
	} else {
		script.to_str().context("script path is not valid UTF-8")?
	};
	let mut info = read_session_info(session_path)?;
	info.last_script = Some(stored_path.to_string());
	write_session_info(session_path, &info)
}

fn read_session_info(session_path: &Path) -> Result<SessionInfo> {
	let path = session_path.join(SESSION_INFO_FILE);
	let text = match fs::read_to_string(&path) {
		Ok(text) => text,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
			return Ok(SessionInfo::default());
		},
		Err(error) => {
			return Err(error).with_context(|| format!("read session metadata {}", path.display()));
		},
	};
	serde_json::from_str(&text)
		.with_context(|| format!("decode session metadata {}", path.display()))
}

fn write_session_info(session_path: &Path, info: &SessionInfo) -> Result<()> {
	fs::create_dir_all(session_path)
		.with_context(|| format!("create session {}", session_path.display()))?;
	let path = session_path.join(SESSION_INFO_FILE);
	let temporary = session_path.join(format!(".{SESSION_INFO_FILE}.{}.tmp", std::process::id()));
	let text = serde_json::to_string_pretty(info)?;
	fs::write(&temporary, format!("{text}\n"))
		.with_context(|| format!("write session metadata {}", temporary.display()))?;
	fs::rename(&temporary, &path)
		.with_context(|| format!("persist session metadata {}", path.display()))?;
	Ok(())
}

fn read_paired_host_store(session_path: &Path) -> Result<PairedHostStore> {
	let path = session_path.join(PAIRED_HOSTS_FILE);
	let text = match fs::read_to_string(&path) {
		Ok(text) => text,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
			return Ok(PairedHostStore::default());
		},
		Err(error) => {
			return Err(error).with_context(|| format!("read paired hosts {}", path.display()));
		},
	};
	let store: PairedHostStore = serde_json::from_str(&text)
		.with_context(|| format!("decode paired hosts {}", path.display()))?;
	anyhow::ensure!(
		store.version == PAIRED_HOST_STORE_VERSION,
		"unsupported paired host store version {} in {}",
		store.version,
		path.display()
	);
	for paired_host in &store.paired_hosts {
		anyhow::ensure!(
			paired_host.version == PAIRED_HOST_VERSION,
			"unsupported paired host version {} in {}",
			paired_host.version,
			path.display()
		);
	}
	let mut statement_account_ids = store
		.paired_hosts
		.iter()
		.map(PairedHost::statement_account_id)
		.collect::<Vec<_>>();
	statement_account_ids.sort();
	let original_len = statement_account_ids.len();
	statement_account_ids.dedup();
	anyhow::ensure!(
		statement_account_ids.len() == original_len,
		"duplicate paired host statement account ID in {}",
		path.display()
	);
	Ok(store)
}

fn write_paired_host_store(session_path: &Path, store: &PairedHostStore) -> Result<()> {
	fs::create_dir_all(session_path)
		.with_context(|| format!("create session {}", session_path.display()))?;
	let path = session_path.join(PAIRED_HOSTS_FILE);
	let temporary = session_path.join(format!(".{PAIRED_HOSTS_FILE}.{}.tmp", std::process::id()));
	let text = serde_json::to_string_pretty(store)?;
	fs::write(&temporary, format!("{text}\n"))
		.with_context(|| format!("write paired hosts {}", temporary.display()))?;
	fs::rename(&temporary, &path)
		.with_context(|| format!("persist paired hosts {}", path.display()))?;
	Ok(())
}

/// A session name cannot safely identify a user-owned profile.
#[derive(Clone, derive_more::Display, derive_more::Error, Debug, PartialEq)]
pub enum SessionNameError {
	/// Names must fit a portable path component.
	#[display("session name must contain between 1 and 64 characters")]
	Length,
	/// A profile cannot start with a separator or uppercase letter.
	#[display("session name must start with a lowercase letter or digit")]
	FirstCharacter,
	/// Only portable lowercase profile names are accepted.
	#[display("session name may contain only lowercase letters, digits, `.`, `_`, and `-`")]
	Character,
	/// The bootstrap profile is not a user-selectable session.
	#[display(
		"session name `default` is reserved for bootstrap state; choose a user session name such as `alice`"
	)]
	Reserved,
}

/// Validate a portable session name before using it as a path component.
pub fn validate_name(name: &str) -> Result<(), SessionNameError> {
	if name.is_empty() || name.len() > 64 {
		return Err(SessionNameError::Length);
	}
	let mut bytes = name.bytes();
	let first = bytes.next().expect("validated non-empty session name");
	if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
		return Err(SessionNameError::FirstCharacter);
	}
	if !bytes.all(|byte| {
		byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
	}) {
		return Err(SessionNameError::Character);
	}
	Ok(())
}

/// Validate a user-selectable session name.
///
/// `default` remains a private bootstrap profile and must not be exposed as a
/// session users can create or switch to.
pub fn validate_selectable_name(name: &str) -> Result<(), SessionNameError> {
	validate_name(name)?;
	if name == DEFAULT_SESSION_NAME {
		return Err(SessionNameError::Reserved);
	}
	Ok(())
}

/// Select the Lite username prefix for auto-accounts owned by a session.
///
/// Lite username bases accept lowercase ASCII letters only, while session
/// names additionally accept digits and separators. Preserve the recognizable
/// alphabetic portion without substituting another base. New accounts require
/// at least six letters. The default session retains the account manager's
/// historical default.
pub fn lite_username_prefix(name: &str) -> Option<String> {
	if name == DEFAULT_SESSION_NAME {
		return None;
	}
	let prefix: String = name.bytes().filter(u8::is_ascii_lowercase).map(char::from).collect();
	Some(prefix)
}

fn username_base(name: &str) -> Option<&str> {
	name.rsplit_once('.').filter(|_| is_lite_label(name)).map(|(base, _)| base)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
	if path.is_absolute() {
		return Ok(path);
	}
	Ok(std::env::current_dir().context("resolve current directory")?.join(path))
}

#[cfg(test)]
mod tests;
