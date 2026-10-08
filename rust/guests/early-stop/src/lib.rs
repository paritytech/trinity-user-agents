//! Test worker: stops a subscription after its first item, then makes one
//! more call, so the host can check the stop arrived while the worker still
//! runs.

use futures::StreamExt;
use truapi::latest::RemotePreimageLookupSubscribeRequest;

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
    let key = b"key".to_vec();
    let mut lookup =
        truapi::preimage::lookup_subscribe(RemotePreimageLookupSubscribeRequest { key });
    lookup
        .next()
        .await
        .ok_or("lookup ended without an item")??;
    drop(lookup);
    truapi::account::get_user_id(()).await?;
    Ok(())
}
