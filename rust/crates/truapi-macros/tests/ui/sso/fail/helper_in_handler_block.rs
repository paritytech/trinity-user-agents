struct Service;

#[truapi_macros::sso_service]
impl Service {
    fn new() -> Self {
        Self
    }
}

fn main() {}
