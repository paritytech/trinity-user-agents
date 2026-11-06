//! Browser bootstrap shared by the native hosts.

/// Render the endpoint configuration consumed by the host container.
pub fn script(url: &str) -> String {
	let url = serde_json::to_string(url).expect("a string always serializes");
	format!("window.__truapi_localhost = {{ url: {url} }};")
}

#[cfg(test)]
mod tests;
