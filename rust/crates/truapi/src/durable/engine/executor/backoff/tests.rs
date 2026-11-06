use super::*;

#[test]
fn the_backoff_doubles_from_two_seconds_up_to_two_minutes() {
	let waits: Vec<u64> = (1..=9).map(|rounds| backoff(rounds).as_secs()).collect();

	assert_eq!(waits, vec![2, 4, 8, 16, 32, 64, 120, 120, 120]);
	assert_eq!(backoff(u32::MAX), MAX_BACKOFF);
}

#[test]
fn the_rebuild_cooldown_doubles_from_five_seconds_up_to_ten_minutes() {
	let waits: Vec<u64> = (0..=9).map(|rebuilds| cooldown(rebuilds).as_secs()).collect();

	assert_eq!(waits, vec![0, 5, 10, 20, 40, 80, 160, 320, 600, 600]);
	assert_eq!(cooldown(u32::MAX), MAX_REBUILD_COOLDOWN);
}
