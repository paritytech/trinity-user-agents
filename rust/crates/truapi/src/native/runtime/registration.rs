//! Session-fenced lite-person registration through the identity backend.

use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::{Client, RequestBuilder};
use serde_json::{Value, json};
use tokio::task::JoinSet;

use super::NativeTrUApiHostRuntime;
use crate::{
	LocalIdentityContext, SigningHostRuntime,
	host_logic::dotns_gateway::is_dotted_lite_username,
	native::{errors::HostRejection, executor::shared_native_executor},
};

#[uniffi::export]
impl NativeTrUApiHostRuntime {
	/// Submit a lite username for the active local identity and return the backend's assignment.
	///
	/// `backend_url` includes the API prefix, such as `/api/v1`. Proofs use the
	/// configured network's People and Asset Hub signing contexts and chain time.
	/// Preferred digits remain a string, including leading zeroes. A returned
	/// assignment is not proof of ownership: the host must confirm it on chain.
	/// Disconnecting or replacing the local session rejects in-flight results.
	pub async fn register_lite_username(
		&self,
		username_base: String,
		preferred_digits: Option<String>,
		backend_url: String,
	) -> Result<String, HostRejection> {
		let context = self.runtime.local_identity_context()?;
		let runtime = self.runtime.clone();
		let executor = shared_native_executor().map_err(rejected)?;
		// Dropping the foreign future drops the set and aborts the HTTP work;
		// a detached task could otherwise submit after the caller withdrew.
		let mut tasks = JoinSet::new();
		tasks.spawn_on(
			async move {
				tokio::time::timeout(
					Duration::from_secs(120),
					register(runtime, context, username_base, preferred_digits, backend_url),
				)
				.await
				.map_err(|_| rejected("identity registration timed out"))?
			},
			&executor.handle(),
		);
		tasks
			.join_next()
			.await
			.ok_or_else(|| rejected("identity registration task did not return"))?
			.map_err(rejected)?
	}
}

async fn register(
	runtime: Arc<SigningHostRuntime>,
	context: LocalIdentityContext,
	username_base: String,
	preferred_digits: Option<String>,
	backend_url: String,
) -> Result<String, HostRejection> {
	let backend = backend_url.trim_end_matches('/');
	let client = Client::builder()
		.timeout(Duration::from_secs(30))
		.redirect(reqwest::redirect::Policy::none())
		.build()
		.map_err(rejected)?;
	let verifier = request_json(
		&runtime,
		&context,
		client.get(format!("{backend}/attester")),
		"attester lookup",
	)
	.await?;
	let verifier_hex = field(&verifier, "attester")?;
	let mut verifier_account = [0u8; 32];
	hex::decode_to_slice(
		verifier_hex.strip_prefix("0x").unwrap_or(verifier_hex),
		&mut verifier_account,
	)
	.map_err(|error| rejected(format!("invalid identity backend attester: {error}")))?;
	let body = runtime
		.local_lite_registration_body(&context.activation_id, &username_base, verifier_account)
		.await?;
	ensure_current(&runtime, &context)?;
	let mut body: Value = serde_json::from_str(&body).map_err(rejected)?;
	if let Some(digits) = preferred_digits {
		body["preferredDigits"] = Value::String(digits);
	}

	let challenge_response = request_json(
		&runtime,
		&context,
		client.post(format!("{backend}/auth/challenges")).json(&json!({})),
		"authentication challenge",
	)
	.await?;
	let challenge = field(&challenge_response, "challenge")?;
	let challenge_bytes = BASE64.decode(challenge).map_err(rejected)?;
	let proof = runtime.local_identity_auth_proof(&context.activation_id, &challenge_bytes)?;
	let (client_id, signature) = proof
		.split_at_checked(32)
		.filter(|(_, signature)| signature.len() == 64)
		.ok_or_else(|| rejected("invalid local identity authentication proof"))?;
	let token_response = request_json(
		&runtime,
		&context,
		client
			.post(format!("{backend}/auth/token"))
			.header("Auth-ClientId", BASE64.encode(client_id))
			.header("Auth-ClientProof", BASE64.encode(signature))
			.header("Auth-Challenge", challenge)
			.header(reqwest::header::CONTENT_TYPE, "application/json")
			.body("{}"),
		"authentication token",
	)
	.await?;
	let response = request_json(
		&runtime,
		&context,
		client
			.post(format!("{backend}/usernames"))
			.bearer_auth(field(&token_response, "token")?)
			.json(&body),
		"lite username registration",
	)
	.await?;
	assigned_username(&response, &username_base)
}

fn ensure_current(
	runtime: &SigningHostRuntime,
	context: &LocalIdentityContext,
) -> Result<(), HostRejection> {
	let current = runtime.local_identity_context()?;
	if current.activation_id != context.activation_id ||
		current.identity_account_id != context.identity_account_id
	{
		return Err(rejected("local identity activation changed"));
	}
	Ok(())
}

async fn request_json(
	runtime: &SigningHostRuntime,
	context: &LocalIdentityContext,
	request: RequestBuilder,
	operation: &str,
) -> Result<Value, HostRejection> {
	ensure_current(runtime, context)?;
	let response = request
		.send()
		.await
		.map_err(|error| rejected(format!("identity backend {operation} failed: {error}")))?;
	ensure_current(runtime, context)?;
	let status = response.status();
	if !status.is_success() {
		return Err(rejected(format!("identity backend {operation} failed (HTTP {status})")));
	}
	let body = response.json().await.map_err(|error| {
		rejected(format!("invalid identity backend {operation} response: {error}"))
	})?;
	ensure_current(runtime, context)?;
	Ok(body)
}

fn assigned_username(response: &Value, requested_base: &str) -> Result<String, HostRejection> {
	let username = field(response, "username")?;
	if !is_dotted_lite_username(username) ||
		username.rsplit_once('.').map(|(base, _)| base) != Some(requested_base)
	{
		return Err(rejected("identity backend returned an invalid lite username assignment"));
	}
	Ok(username.to_owned())
}

fn field<'a>(body: &'a Value, name: &str) -> Result<&'a str, HostRejection> {
	body.get(name)
		.and_then(Value::as_str)
		.filter(|value| !value.is_empty())
		.ok_or_else(|| rejected(format!("identity backend response is missing {name}")))
}

fn rejected(reason: impl ToString) -> HostRejection {
	HostRejection::Rejected { reason: reason.to_string() }
}

#[cfg(test)]
mod tests;
