//! Product-facing call media capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::CallMedia,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		call_media::{
			HostCallMediaError, HostCallMediaResponse, HostCallMediaSignalRequest,
			HostCallMediaStartRequest, HostCallMediaStopRequest,
		},
	},
};

use super::from_latest_error;
use crate::{
	platform::{CHAT_CALL_SIGNAL_MAX_BYTES, normalize_chat_identifier},
	runtime::{ProductRuntimeHost, until_cancelled},
};

/// The media of a call starts only this long after the user pressed a call
/// button or accepted a call in the room: the ring of the other side and the
/// acceptance fit in it.
pub const CALL_MEDIA_CONSENT_SECS: u64 = 180;

/// The longest call id.
pub const CALL_ID_MAX_BYTES: usize = 128;

fn invalid(reason: String) -> CallError<latest::HostCallMediaError> {
	CallError::Domain(latest::HostCallMediaError::InvalidRequest { reason })
}

fn unknown(reason: String) -> CallError<latest::HostCallMediaError> {
	CallError::Domain(latest::HostCallMediaError::Unknown { reason })
}

/// The room id and the call id of a request, checked.
fn call_of(
	room_id: &str,
	call_id: &str,
) -> Result<(String, String), CallError<latest::HostCallMediaError>> {
	let room_id =
		normalize_chat_identifier("roomId", room_id).map_err(|error| invalid(error.to_string()))?;
	if call_id.is_empty() || call_id.len() > CALL_ID_MAX_BYTES {
		return Err(invalid(format!(
			"the call id is empty or longer than {CALL_ID_MAX_BYTES} bytes"
		)));
	}
	Ok((room_id, call_id.to_string()))
}

#[truapi::async_trait]
impl CallMedia for ProductRuntimeHost {
	/// Checked in this order so a host without call media, or a request the
	/// product could not make, starts no microphone or camera.
	#[instrument(skip_all, fields(runtime.method = "callMedia.start"))]
	async fn start(
		&self,
		cx: &CallContext,
		request: HostCallMediaStartRequest,
	) -> Result<HostCallMediaResponse, CallError<HostCallMediaError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform =
				self.connection.services.call_media_platform().ok_or(CallError::Unsupported)?;
			let (room_id, call_id) = call_of(&request.room_id, &request.call_id)?;
			let consented = self.connection.services.chat_room_events.call_consented_within(
				self.connection.product.product_id.as_str(),
				&room_id,
				CALL_MEDIA_CONSENT_SECS,
			);
			if !consented {
				return Err(CallError::Domain(latest::HostCallMediaError::NotAllowed));
			}
			until_cancelled(
				cx,
				platform.start_call_media(
					&self.connection.product,
					&room_id,
					&call_id,
					request.video,
					request.offerer,
				),
			)
			.await
			.map_err(|error| unknown(error.to_string()))?
			.map_err(|error| unknown(error.reason))?;
			Ok(latest::HostCallMediaResponse {})
		}
		.await;
		result
			.map(|response| HostCallMediaResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "callMedia.signal"))]
	async fn signal(
		&self,
		cx: &CallContext,
		request: HostCallMediaSignalRequest,
	) -> Result<HostCallMediaResponse, CallError<HostCallMediaError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform =
				self.connection.services.call_media_platform().ok_or(CallError::Unsupported)?;
			let (room_id, call_id) = call_of(&request.room_id, &request.call_id)?;
			if request.signal.is_empty() || request.signal.len() > CHAT_CALL_SIGNAL_MAX_BYTES {
				return Err(invalid(format!(
					"the signal is empty or longer than {CHAT_CALL_SIGNAL_MAX_BYTES} bytes"
				)));
			}
			until_cancelled(
				cx,
				platform.call_media_signal(
					&self.connection.product,
					&room_id,
					&call_id,
					request.signal,
				),
			)
			.await
			.map_err(|error| unknown(error.to_string()))?
			.map_err(|error| unknown(error.reason))?;
			Ok(latest::HostCallMediaResponse {})
		}
		.await;
		result
			.map(|response| HostCallMediaResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "callMedia.stop"))]
	async fn stop(
		&self,
		cx: &CallContext,
		request: HostCallMediaStopRequest,
	) -> Result<HostCallMediaResponse, CallError<HostCallMediaError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform =
				self.connection.services.call_media_platform().ok_or(CallError::Unsupported)?;
			let (room_id, call_id) = call_of(&request.room_id, &request.call_id)?;
			until_cancelled(
				cx,
				platform.stop_call_media(&self.connection.product, &room_id, &call_id),
			)
			.await
			.map_err(|error| unknown(error.to_string()))?
			.map_err(|error| unknown(error.reason))?;
			Ok(latest::HostCallMediaResponse {})
		}
		.await;
		result
			.map(|response| HostCallMediaResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}
