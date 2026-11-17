use super::bundled::paseo_next_v2;

/// The first RPC endpoint of the bundled paseo-next-v2 chain with `role`.
pub fn paseo_next_v2_rpc(role: &str) -> String {
	paseo_next_v2()
		.chain(role)
		.expect("bundled paseo-next-v2 stack lacks the role")
		.rpc[0]
		.clone()
}
