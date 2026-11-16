//! Installing the native route on the runtime services, and keeping what it
//! offers in line with the chain.

use std::sync::Arc;

use super::{
	session::{NativeFunding, NativeLink},
	tokens::native_manifest,
};
use crate::{platform::ProductId, runtime::RuntimeServices, unix_time::current_unix_millis};

impl RuntimeServices {
	/// Install `route` as the core's native funding route: link it as the
	/// native provider, which the registry hands every session it serves on
	/// that link, and read what it serves. Set-once; returns whether this
	/// call installed it.
	pub(crate) fn install_native_funding(self: &Arc<Self>, route: Arc<NativeFunding>) -> bool {
		if !self.keep_native_funding(route.clone()) {
			return false;
		}
		let native = ProductId::native_funding();
		self.funding().link(&native, Arc::new(NativeLink(route)));
		self.funding().connected(&native);
		self.refresh_native_funding();
		true
	}

	/// Read live which tokens the native route serves, at most once a
	/// minute, and offer it as serving them: first among the providers while
	/// it serves any, not at all while it serves none.
	pub(crate) fn refresh_native_funding(self: &Arc<Self>) {
		let Some(route) = self.native_funding().cloned() else {
			return;
		};
		if !route.begin_check(current_unix_millis()) {
			return;
		}
		let services = self.clone();
		self.spawner.spawn(async move {
			let tokens = route.tokens_in().await;
			route.end_check(current_unix_millis());
			if services.funding_providers().set_native(native_manifest(&tokens)) {
				services.funding_providers_changed();
			}
		});
	}
}
