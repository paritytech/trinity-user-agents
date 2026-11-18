use std::sync::Arc;

use super::{
	AccountHolder, HostAccounts, HostSession, PairingHostRole, RemoteAccountHolder, SigningHostRole,
	signing_host::WalletAccountHolder,
};

/// The accounts and session that a host shares with its product connections.
pub trait HostRole {
	/// Holder used for product account requests.
	type Holder: AccountHolder;

	/// Runtime name recorded in host method spans.
	fn trace_method(method: &str) -> String;

	/// Account policy and grants for this host.
	fn accounts(&self) -> &Arc<HostAccounts<Self::Holder>>;

	/// Session used for product login, identity and disconnection.
	fn host_session(self: &Arc<Self>) -> Arc<dyn HostSession>;
}

impl HostRole for PairingHostRole {
	type Holder = RemoteAccountHolder;

	fn trace_method(method: &str) -> String {
		format!("pairing_host_runtime.{method}")
	}

	fn accounts(&self) -> &Arc<HostAccounts<Self::Holder>> {
		self.accounts()
	}

	fn host_session(self: &Arc<Self>) -> Arc<dyn HostSession> {
		self.session().clone()
	}
}

impl HostRole for SigningHostRole {
	type Holder = WalletAccountHolder;

	fn trace_method(method: &str) -> String {
		format!("signing_host_runtime.{method}")
	}

	fn accounts(&self) -> &Arc<HostAccounts<Self::Holder>> {
		self.accounts()
	}

	fn host_session(self: &Arc<Self>) -> Arc<dyn HostSession> {
		self.clone()
	}
}
