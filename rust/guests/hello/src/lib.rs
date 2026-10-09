//! Logs the signed-in user's id.

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
    let user = truapi::account::get_user_id(()).await?;
    truapi::log!("user id: {}", user.primary_username);
    Ok(())
}
