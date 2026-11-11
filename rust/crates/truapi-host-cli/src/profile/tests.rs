use std::collections::VecDeque;

use super::*;

fn avatar_entry(seed: u8) -> WrapperKeepOpaque<ProfileEntry> {
	WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
		cid: preimage_key(&[seed]).into(),
		format: ImageFormat::Png,
	}))
}

#[test]
fn new_avatar_replaces_the_one_readers_show_and_keeps_other_writers_entries() {
	let unknown = WrapperKeepOpaque::from_encoded(vec![7, 1, 2]);
	let record = ProfileRecord { entries: vec![unknown.clone(), avatar_entry(1), avatar_entry(2)] };

	let updated = with_avatar(
		record,
		ProfileAvatar { cid: preimage_key(&[3]).into(), format: ImageFormat::Png },
	);

	assert_eq!(updated, ProfileRecord { entries: vec![avatar_entry(3), unknown, avatar_entry(2)] });
}

#[derive(Debug, PartialEq)]
enum Operation {
	Read,
	Prepare(String),
	Upload(Vec<u8>),
	Publish(String),
}

#[derive(Default)]
struct Backend {
	records: VecDeque<Option<String>>,
	operations: Vec<Operation>,
}

#[async_trait]
impl ProfileBackend for Backend {
	type Prepared = String;

	async fn read(&mut self) -> Result<Option<String>> {
		self.operations.push(Operation::Read);
		Ok(self.records.pop_front().flatten())
	}

	async fn prepare(&mut self, text: &str) -> Result<String> {
		self.operations.push(Operation::Prepare(text.to_string()));
		Ok(text.to_string())
	}

	async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]> {
		let key = preimage_key(&bytes);
		self.operations.push(Operation::Upload(bytes));
		Ok(key)
	}

	async fn publish(&mut self, prepared: String) -> Result<()> {
		self.records.push_back(Some(prepared.clone()));
		self.operations.push(Operation::Publish(prepared));
		Ok(())
	}
}

const IMAGE: [u8; 3] = [1, 2, 3];

#[tokio::test]
async fn the_record_is_written_only_after_its_avatar_is_stored_and_confirmed_after() {
	let mut backend = Backend { records: [None, None].into(), ..Default::default() };

	let record = publish_avatar(&mut backend, (ImageFormat::Png, IMAGE.to_vec())).await.unwrap();

	let text = record.to_text().unwrap();
	assert_eq!(
		(record.avatar(), backend.operations),
		(
			Some(ProfileAvatar { cid: preimage_key(&IMAGE).into(), format: ImageFormat::Png }),
			vec![
				Operation::Read,
				Operation::Prepare(text.clone()),
				Operation::Upload(IMAGE.to_vec()),
				Operation::Read,
				Operation::Publish(text),
				Operation::Read,
			]
		)
	);
}

#[tokio::test]
async fn a_record_changed_during_the_upload_is_not_overwritten() {
	let changed = ProfileRecord { entries: vec![avatar_entry(9)] }.to_text();
	let mut backend = Backend { records: [None, changed].into(), ..Default::default() };

	let error = publish_avatar(&mut backend, (ImageFormat::Png, IMAGE.to_vec()))
		.await
		.unwrap_err();

	assert_eq!(
		(error.to_string(), backend.operations.last()),
		(
			"profile changed during the upload; retry to preserve the new record".to_string(),
			Some(&Operation::Read)
		)
	);
}
