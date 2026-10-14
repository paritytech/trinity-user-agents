#![deny(dead_code, unreachable_patterns)]

include!("../support/wire.rs");
include!("../support/runtime.rs");

use host_internal::{
    sso_messages::*,
    sso_wire::{ResponseOutcome, SsoRequest},
};
use runtime::sso_service::{Dispatch, SsoReply, SsoRequestContext};

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
    assert_eq!(Request::<u32>::NAME, "foo");
    assert_eq!(BarRequest::NAME, "bar");
    assert_eq!(Box::<Request<bool>>::NAME, "baz");

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
    assert_eq!(
        Request::<u32>::response_from_message(response),
        Some(Response {
            responding_to: "m-3".into(),
            payload: Ok(8),
        }),
    );

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

    let service = Service::new();
    let Dispatch::Response(answer) = dispatch(&service, Some(SsoRequestContext), request) else {
        panic!("a request must produce a response");
    };
    assert_eq!(
        Request::<u32>::response_from_message(answer.0),
        Some(Response {
            responding_to: "m-dispatch".into(),
            payload: Ok(1),
        }),
    );

    let Dispatch::Response(answer) = dispatch(&service, None, BarRequest.into_message()) else {
        panic!("a request without a session must produce an error response");
    };
    assert_eq!(
        BarRequest::response_from_message(answer.0),
        Some(Response {
            responding_to: "m-dispatch".into(),
            payload: Err("disconnected".into()),
        }),
    );

    let response = Request::<u32>::response_into_message(Response {
        responding_to: "m-1".into(),
        payload: Ok(7),
    });
    let Dispatch::NotARequest(name) = dispatch(&service, None, response) else {
        panic!("an incoming response must not be dispatched as a request");
    };
    assert_eq!(name, "BarResponse");

    let Dispatch::Withdraw(message_id) = dispatch(&service, None, withdrawal) else {
        panic!("a cancellation must withdraw the original request");
    };
    assert_eq!(message_id, "m-1");
    assert!(matches!(
        dispatch(&service, None, v1::RemoteMessage::Disconnected),
        Dispatch::Disconnected,
    ));
}

fn dispatch(
    service: &Service,
    context: Option<SsoRequestContext>,
    message: v1::RemoteMessage,
) -> Dispatch {
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    fn run(future: impl Future<Output = Dispatch> + Send) -> Dispatch {
        match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("fixture handlers must complete immediately"),
        }
    }

    run(service.dispatch(
        context,
        RemoteMessage {
            message_id: "m-dispatch".into(),
            data: RemoteMessageData::V1(message),
        },
    ))
}
