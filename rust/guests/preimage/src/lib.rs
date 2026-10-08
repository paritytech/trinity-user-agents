//! Submits a preimage, then waits on a lookup subscription until it resolves.

use futures::StreamExt;
use truapi_guest_api::latest::RemotePreimageLookupSubscribeRequest;
use truapi_guest_api::{Error, log, preimage};

#[truapi_guest_api::main]
async fn main() -> Result<(), Error> {
    let value = b"hello truapi".to_vec();
    let key = preimage::submit(value.clone()).await?;

    let mut lookup = preimage::lookup_subscribe(RemotePreimageLookupSubscribeRequest { key });
    while let Some(item) = lookup.next().await {
        if item?.value.as_deref() == Some(value.as_slice()) {
            log!("preimage resolved");
            return Ok(());
        }
    }
    Err("lookup ended before the preimage resolved".into())
}
