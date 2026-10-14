#![deny(unreachable_patterns)]

include!("../support/wire.rs");
include!("../support/runtime.rs");

use host_internal::{
    sso_messages::*,
    sso_wire::{ResponseOutcome, SsoRequest},
};
use runtime::sso_service::{SsoReply, SsoRequestContext};

struct Service;

impl Service {
    fn new() -> Self {
        Self
    }

    async fn value(&self, value: u32) -> Result<u32, String> {
        Ok(value)
    }
}

#[truapi_macros::sso_service]
impl Service {
    async fn r#foo(&self, _: &SsoRequestContext, Request(value): Request<u32>) -> BarResponse {
        let value = self.value(value).await?;
        if value == 0 {
            return Err("zero".into());
        }
        Ok(value)
    }

    async fn bar(&self, _: &SsoRequestContext, _: BarRequest) -> FooResponse {
        SsoReply::<FooResponse>::from(Ok(2)).with_outcome(ResponseOutcome)
    }

    async fn baz(&self, _: &SsoRequestContext, _: Box<Request<bool>>) -> BarResponse {
        Ok(3)
    }
}

fn main() {
    // The return type selects the variant, even when its name differs from the handler's.
    let response = Request::<u32>::response_into_message(Response {
        responding_to: "m-1".into(),
        payload: Ok(7),
    });
    assert!(matches!(response, v1::RemoteMessage::BarResponse(_)));
    assert!(BarRequest::response_from_message(response).is_none());

    // Two handlers can share a response without duplicating generated match arms.
    let response = Box::<Request<bool>>::response_into_message(Response {
        responding_to: "m-2".into(),
        payload: Ok(8),
    });
    assert_eq!(response.name(), "BarResponse");
    assert_eq!(response.responding_to(), Some("m-2"));
    let response = response.with_responding_to("m-3".into());
    assert_eq!(response.responding_to(), Some("m-3"));
    assert!(Request::<u32>::response_from_message(response).is_some());

    let request = Request(1_u32).into_message();
    assert_eq!(request.name(), "foo");
    assert_eq!(request.responding_to(), None);
    assert!(matches!(request, v1::RemoteMessage::FooRequest(Request(1))));
    assert_eq!(v1::RemoteMessage::Disconnected.name(), "Disconnected");
    assert_eq!(v1::RemoteMessage::Disconnected.responding_to(), None);
    let withdrawal = v1::RemoteMessage::Cancel(v1::Withdrawal {
        message_id: "m-1".into(),
    });
    assert_eq!(withdrawal.name(), "Cancel");

    fn require_send(_: impl core::future::Future + Send) {}
    let service = Service::new();
    require_send(service.dispatch(
        None,
        RemoteMessage {
            message_id: "m-1".into(),
            data: RemoteMessageData::V1(request),
        },
    ));
}
