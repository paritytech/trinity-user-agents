//! Submits a preimage, then waits on a lookup subscription until it resolves.

use futures::StreamExt;
use truapi::latest::RemotePreimageLookupSubscribeRequest;

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
    let value = b"hello truapi".to_vec();
    let key = truapi::preimage::submit(value.clone()).await?;

    let mut lookup =
        truapi::preimage::lookup_subscribe(RemotePreimageLookupSubscribeRequest { key });
    while let Some(item) = lookup.next().await {
        if item?.value.as_deref() == Some(value.as_slice()) {
            truapi::log!("preimage resolved");
            return Ok(());
        }
    }
    Err("lookup ended before the preimage resolved".into())
}
