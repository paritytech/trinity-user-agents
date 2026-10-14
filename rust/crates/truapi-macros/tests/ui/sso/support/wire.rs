// Minimal server wire contract for testing macro expansions independently of runtime I/O.
#[allow(dead_code)]
mod host_internal {
    pub mod sso_wire {
        use super::sso_messages::{Response, v1::RemoteMessage};

        pub trait SsoRequest: Sized {
            const NAME: &'static str;
            type Response;
            fn into_message(self) -> RemoteMessage;
            fn response_into_message(response: Response<Self::Response>) -> RemoteMessage;
            fn response_from_message(message: RemoteMessage) -> Option<Response<Self::Response>>;
        }

        pub trait SsoError: core::fmt::Display {
            fn not_connected() -> Self;
        }

        impl SsoError for String {
            fn not_connected() -> Self {
                "disconnected".into()
            }
        }

        pub struct ResponseOutcome;
    }

    pub mod sso_messages {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Request<T>(pub T);

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct BarRequest;

        pub struct Response<P> {
            pub responding_to: String,
            pub payload: P,
        }

        pub type FooResponse = Result<u32, String>;
        pub type BarResponse = Result<u32, String>;

        pub struct RemoteMessage {
            pub message_id: String,
            pub data: RemoteMessageData,
        }

        pub enum RemoteMessageData {
            V1(v1::RemoteMessage),
        }

        pub mod v1 {
            use super::*;

            pub enum RemoteMessage {
                Disconnected,
                FooRequest(Request<u32>),
                FooResponse(Response<FooResponse>),
                BarRequest(BarRequest),
                BarResponse(Response<BarResponse>),
                BazRequest(Box<Request<bool>>),
                Cancel(Withdrawal),
            }

            pub struct Withdrawal {
                pub message_id: String,
            }
        }
    }
}
