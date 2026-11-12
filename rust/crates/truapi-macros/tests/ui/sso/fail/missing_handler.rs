include!("../support/wire.rs");
include!("../support/runtime.rs");

use host_internal::sso_messages::{BarResponse, FooResponse, Request};
use runtime::sso_service::SsoRequestContext;

struct Service;

#[truapi_macros::sso_service]
impl Service {
    async fn foo(&self, _: &SsoRequestContext, _request: Request<u32>) -> FooResponse {
        Ok(1)
    }

    async fn baz(&self, _: &SsoRequestContext, _request: Box<Request<bool>>) -> BarResponse {
        Ok(3)
    }
}

fn main() {}
