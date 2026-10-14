// Compile-time contracts consumed by generated dispatch; runtime behavior is tested in the server.
#[allow(dead_code)]
mod runtime {
    pub mod authority {
        pub struct AuthoritySession;
    }

    pub mod sso_service {
        use crate::host_internal::sso_messages::{Response, v1};
        use crate::host_internal::sso_wire::ResponseOutcome;

        pub struct SsoRequestContext;

        impl SsoRequestContext {
            pub fn new(_: &str, _: super::authority::AuthoritySession) -> Self {
                Self
            }
        }

        pub enum Dispatch {
            Response(Box<Answer>),
            Disconnected,
            NotARequest(&'static str),
            Withdraw(String),
        }

        pub struct Answer;

        pub struct SsoReply<P>(P);

        impl<P> From<P> for SsoReply<P> {
            fn from(payload: P) -> Self {
                Self(payload)
            }
        }

        impl<P> SsoReply<P> {
            pub fn with_outcome(self, _: ResponseOutcome) -> Self {
                self
            }
        }

        impl<T, E: core::fmt::Display> SsoReply<Result<T, E>> {
            pub fn finish(
                self,
                _: &str,
                _: impl FnOnce(Response<Result<T, E>>) -> v1::RemoteMessage,
            ) -> Answer {
                Answer
            }
        }
    }
}
