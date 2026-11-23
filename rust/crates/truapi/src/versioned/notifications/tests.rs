use super::*;

#[test]
fn a_v01_push_notification_upgrades_without_a_title() {
	let upgraded = HostPushNotificationRequest::V1(v01::HostPushNotificationRequest {
		text: "hello".to_string(),
		deeplink: Some("https://dim2.dot".to_string()),
		scheduled_at: Some(5),
	})
	.into_latest();
	assert_eq!(
		upgraded,
		v02::HostPushNotificationRequest {
			title: None,
			text: "hello".to_string(),
			deeplink: Some("https://dim2.dot".to_string()),
			scheduled_at: Some(5),
		}
	);
}
