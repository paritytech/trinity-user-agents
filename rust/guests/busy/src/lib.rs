//! Test worker that never yields, so the host must stop it by fuel.

#[truapi::main]
async fn main() -> Result<(), truapi::Error> {
	let mut count: u64 = 0;
	loop {
		count = core::hint::black_box(count.wrapping_add(1));
	}
}
