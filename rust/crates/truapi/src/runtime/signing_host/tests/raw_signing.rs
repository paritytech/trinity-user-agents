//! Signature and consent regression coverage, including the deprecated APIs.
#![allow(deprecated)]

use super::*;
use crate::{
	host_internal::sso_messages::{
		RemoteMessage, RemoteMessageData, SignRawWithLegacyAccountRequest, SignRequest, v1,
	},
	platform::SignRawReview,
	runtime::{signing_host::sso_service::SigningHostSsoService, sso_service::Dispatch},
};
use parity_scale_codec::{Decode, Encode};
use truapi::versioned::signing::{
	HostSignRawWithLegacyAccountRequest, HostSignRawWithLegacyAccountResponse,
};

#[test]
fn raw_signing_review_matches_the_signed_bytes() {
	let platform = Arc::new(StubPlatform { sign_raw_confirmed: true, ..Default::default() });
	let (services, activation) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec())).unwrap();
	let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let product = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
	let runtime = product_runtime(services, activation);
	let cx = CallContext::default();
	for (legacy, watermarked) in [(false, false), (true, false), (false, true), (true, true)] {
		let payload = v01::RawPayload::Bytes { bytes: vec![0x11; 32] };
		let response = futures::executor::block_on(async {
			if legacy {
				let request = HostSignRawWithLegacyAccountRequest::V1(
					v01::HostSignRawWithLegacyAccountRequest {
						signer: subxt::utils::AccountId32(identity.public.to_bytes()).to_string(),
						payload,
					},
				);
				let HostSignRawWithLegacyAccountResponse::V1(response) = if watermarked {
					runtime.sign_raw_with_legacy_account(&cx, request).await
				} else {
					runtime
						.sign_raw_unwatermarked_deprecated_with_legacy_account(&cx, request)
						.await
				}
				.unwrap();
				response
			} else {
				let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
					account: product_account(0),
					payload,
				});
				let HostSignRawResponse::V1(response) = if watermarked {
					runtime.sign_raw(&cx, request).await
				} else {
					runtime.sign_raw_unwatermarked_deprecated(&cx, request).await
				}
				.unwrap();
				response
			}
		});
		let public = if legacy { &identity.public } else { &product.public };
		let signature = schnorrkel::Signature::from_bytes(&response.signature).unwrap();
		assert_eq!(
			public.verify_simple(b"substrate", &[0x11; 32], &signature).is_ok(),
			!watermarked,
		);
		assert_eq!(
			public
				.verify_simple(
					b"substrate",
					&[b"<Bytes>".as_slice(), &[0x11; 32], b"</Bytes>"].concat(),
					&signature,
				)
				.is_ok(),
			watermarked,
		);
		assert_review_watermark(&platform, legacy, watermarked);
	}
}

#[test]
fn paired_raw_signing_review_matches_the_signed_bytes_and_requires_confirmation() {
	for (legacy, watermarked) in [(false, false), (true, false), (false, true), (true, true)] {
		for confirmed in [false, true] {
			let platform =
				Arc::new(StubPlatform { sign_raw_confirmed: confirmed, ..Default::default() });
			let (_, activation) = signing_runtime_with_platform(platform.clone());
			futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
				.unwrap();
			let service = SigningHostSsoService::new(activation);
			let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
			let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
			let product = derive_product_keypair(&root, "myapp.dot", index_bytes(0)).unwrap();
			let payload = v01::RawPayload::Bytes { bytes: vec![0x11; 32] };
			let message = if legacy && watermarked {
				RemoteMessage::request(
					"proof".into(),
					SignRawWithLegacyAccountRequest {
						account: identity.public.to_bytes(),
						data: payload,
					},
				)
			} else {
				let request = if legacy {
					SignRequest::RawWithLegacyAccountUnwatermarkedDeprecated(
						SignRawWithLegacyAccountRequest {
							account: identity.public.to_bytes(),
							data: payload,
						},
					)
				} else {
					let request = v01::HostSignRawRequest { account: product_account(0), payload };
					if watermarked {
						SignRequest::Raw(request)
					} else {
						SignRequest::RawUnwatermarkedDeprecated(request)
					}
				};
				let encoded = request.encode();
				assert_eq!(
					encoded[0],
					if legacy {
						3
					} else if watermarked {
						1
					} else {
						2
					}
				);
				let request = SignRequest::decode(&mut encoded.as_slice()).unwrap();
				RemoteMessage::request("proof".into(), request)
			};
			let Dispatch::Response(answer) = futures::executor::block_on(service.answer(message))
			else {
				panic!("expected signing response")
			};
			let signature = match answer.message.data {
				RemoteMessageData::V1(v1::RemoteMessage::SignResponse(response)) => {
					response.payload.map(|response| response.signature)
				},
				RemoteMessageData::V1(v1::RemoteMessage::SignRawWithLegacyAccountResponse(
					response,
				)) => response.payload,
				_ => panic!("expected raw signing response"),
			};
			assert_review_watermark(&platform, legacy, watermarked);
			if confirmed {
				let signature = schnorrkel::Signature::from_bytes(&signature.unwrap()).unwrap();
				let public = if legacy { &identity.public } else { &product.public };
				assert_eq!(
					public.verify_simple(b"substrate", &[0x11; 32], &signature).is_ok(),
					!watermarked,
				);
				assert_eq!(
					public
						.verify_simple(
							b"substrate",
							&[b"<Bytes>".as_slice(), &[0x11; 32], b"</Bytes>"].concat(),
							&signature,
						)
						.is_ok(),
					watermarked,
				);
			} else {
				assert_eq!(signature.unwrap_err(), "Rejected");
			}
		}
	}
}

fn assert_review_watermark(platform: &StubPlatform, legacy: bool, watermarked: bool) {
	let reviews = std::mem::take(&mut *platform.sign_raw_reviews.lock().unwrap());
	let [review] = reviews.as_slice() else {
		panic!("expected one raw signing review: {reviews:?}");
	};
	let (actual_legacy, actual_watermarked, payload) = match review {
		SignRawReview::Product { request, watermarked, .. } => {
			(false, *watermarked, &request.payload)
		},
		SignRawReview::LegacyAccount { request, watermarked } => {
			(true, *watermarked, &request.payload)
		},
	};
	assert_eq!((actual_legacy, actual_watermarked), (legacy, watermarked));
	assert_eq!(payload, &v01::RawPayload::Bytes { bytes: vec![0x11; 32] });
}
