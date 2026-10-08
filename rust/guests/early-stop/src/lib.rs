//! Test worker: stops a subscription after its first item, then makes one
//! more call, so the host can check the stop arrived while the worker still
//! runs.

use futures::StreamExt;
use truapi_guest_api::latest::RemotePreimageLookupSubscribeRequest;
use truapi_guest_api::{Error, account, preimage};

truapi_guest_api::main!(run);

async fn run() -> Result<(), Error> {
    let key = b"key".to_vec();
    let mut lookup = preimage::lookup_subscribe(RemotePreimageLookupSubscribeRequest { key });
    lookup
        .next()
        .await
        .ok_or("lookup ended without an item")??;
    drop(lookup);
    account::get_user_id(()).await?;
    Ok(())
}
