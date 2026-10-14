include!("../support/wire.rs");
include!("../support/runtime.rs");

use host_internal::sso_messages::*;
use runtime::sso_service::SsoRequestContext;

struct Service;

#[truapi_macros::sso_service]
impl Service {
    async fn foo(&self, _: &SsoRequestContext, _request: Request<u32>) -> FooResponse {
        Ok("wrong payload type")
    }

    async fn bar(&self, _: &SsoRequestContext, _request: BarRequest) -> BarResponse {
        Ok(2)
    }

    async fn baz(&self, _: &SsoRequestContext, _request: Box<Request<bool>>) -> FooResponse {
        Ok(3)
    }
}

fn main() {}
