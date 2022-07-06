#[derive(Debug)]
pub struct FilePart {
	pub file_name: String,
	pub mime_str: String,
	pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct FileParts {
	pub inner: Vec<FilePart>,
}

impl From<Vec<u8>> for FileParts {
	fn from(raw: Vec<u8>) -> FileParts {
		if raw.len() < 16 {
			return FileParts { inner: vec![] };
		}

		let total_len = i32::from_le_bytes((&raw[0..4]).try_into().unwrap()) as usize;
		let mut v = Vec::<FilePart>::with_capacity(total_len);
		let mut v_offset = (1 + (total_len * 3)) * 4;
		for i in 0..total_len {
			let offset = (1 + (i * 3)) * 4;
			let file_name_len =
				i32::from_le_bytes((&raw[offset..offset + 4]).try_into().unwrap()) as usize;
			let file_name = String::from_utf8((&raw[v_offset..v_offset + file_name_len]).to_vec())
				.unwrap_or_default();
			v_offset += file_name_len;

			let mime_str_len =
				i32::from_le_bytes((&raw[offset + 4..offset + 8]).try_into().unwrap()) as usize;
			let mime_str = String::from_utf8((&raw[v_offset..v_offset + mime_str_len]).to_vec())
				.unwrap_or_default();
			v_offset += mime_str_len;

			let bytes_len =
				i32::from_le_bytes((&raw[offset + 8..offset + 12]).try_into().unwrap()) as usize;
			let bytes = (&raw[v_offset..v_offset + bytes_len]).to_vec();
			v_offset += bytes_len;

			v.push(FilePart {
				file_name,
				mime_str,
				bytes,
			});
		}

		FileParts { inner: v }
	}
}

impl FileParts {
	pub fn to_vec(&self) -> Vec<u8> {
		let mut nv = vec![0 as u8; (1 + (3 * self.inner.len())) * 4];
		nv.splice(0..4, (self.inner.len() as i32).to_le_bytes());
		self.inner
			.iter()
			.enumerate()
			.fold(nv, |mut accum, (index, item)| {
				let file_name = item.file_name.as_bytes();
				accum.extend(file_name);
				let mime_str = item.mime_str.as_bytes();
				accum.extend(mime_str);
				accum.extend(&item.bytes);
				let offset = (1 + (index * 3)) * 4;
				accum.splice(offset..offset + 4, (file_name.len() as i32).to_le_bytes());
				accum.splice(
					offset + 4..offset + 8,
					(mime_str.len() as i32).to_le_bytes(),
				);
				accum.splice(
					offset + 8..offset + 12,
					(item.bytes.len() as i32).to_le_bytes(),
				);
				accum
			})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn from_into() {
		let fp = FileParts {
			inner: vec![
				FilePart {
					file_name: String::from("a.txt"),
					mime_str: String::from("text/plain"),
					bytes: b"123".to_vec(),
				},
				FilePart {
					file_name: String::from("g.jpg"),
					mime_str: String::from("image/jpeg"),
					bytes: b"!@#$%^&*()".to_vec(),
				},
			],
		};

		let v = fp.to_vec();
		println!("{:?}", v);

		let fp2: FileParts = v.into();
		println!("{:?}", fp2);

		assert_eq!(fp.inner[0].file_name, fp2.inner[0].file_name);
		assert_eq!(fp.inner[1].bytes, fp2.inner[1].bytes);
	}
}
