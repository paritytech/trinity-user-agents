include!("../support/wire.rs");
include!("../support/runtime.rs");

use host_internal::sso_messages::*;
use runtime::sso_service::SsoRequestContext;

struct Service;

// FooResponse and BarResponse have identical payload types. Every request has
// a handler, but leaving BarResponse unused must still fail wire coverage.
#[truapi_macros::sso_service]
impl Service {
    async fn foo(&self, _: &SsoRequestContext, _: Request<u32>) -> FooResponse {
        Ok(1)
    }

    async fn bar(&self, _: &SsoRequestContext, _: BarRequest) -> FooResponse {
        Ok(2)
    }

    async fn baz(&self, _: &SsoRequestContext, _: Box<Request<bool>>) -> FooResponse {
        Ok(3)
    }
}

fn main() {}
