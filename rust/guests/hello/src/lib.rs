//! Logs the signed-in user's id.

use truapi_guest_api::{Error, account, log};

truapi_guest_api::main!(run);

async fn run() -> Result<(), Error> {
    let user = account::get_user_id(()).await?;
    log!("user id: {}", user.primary_username);
    Ok(())
}
