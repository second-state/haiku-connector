use std::{collections::HashMap, fmt};

pub mod fileparts;

#[derive(Debug)]
pub enum RequestMethod {
	GET,
	POST,
	PUT,
	DELETE,
	UNKNOWN,
}

impl fmt::Display for RequestMethod {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

impl From<u8> for RequestMethod {
	fn from(o: u8) -> Self {
		match o {
			0 => RequestMethod::GET,
			1 => RequestMethod::POST,
			2 => RequestMethod::PUT,
			3 => RequestMethod::DELETE,
			_ => RequestMethod::UNKNOWN,
		}
	}
}

#[link(wasm_import_module = "haiku-connector")]
extern "C" {
	fn send_request(
		url_pointer: i32,
		url_len: i32,
		method: u8,
		headers_pointer: i32,
		headers_len: i32,
		body_pointer: i32,
		body_len: i32,
	) -> i32;
	fn send_async_request(
		url_pointer: i32,
		url_len: i32,
		method: u8,
		headers_pointer: i32,
		headers_len: i32,
		body_pointer: i32,
		body_len: i32,
	);
	fn send_fileparts_request(
		url_pointer: i32,
		url_len: i32,
		method: u8,
		headers_pointer: i32,
		headers_len: i32,
		body_pointer: i32,
		body_len: i32,
		fileparts_pointer: i32,
		fileparts_len: i32,
	) -> i32;
	fn send_async_fileparts_request(
		url_pointer: i32,
		url_len: i32,
		method: u8,
		headers_pointer: i32,
		headers_len: i32,
		body_pointer: i32,
		body_len: i32,
		fileparts_pointer: i32,
		fileparts_len: i32,
	);
}

#[inline(always)]
fn parse_params(
	url: &mut str,
	method: RequestMethod,
	headers: &mut Vec<u8>,
	body: &mut [u8],
) -> Result<(i32, i32, u8, i32, i32, i32, i32), String> {
	unsafe {
		let url = url.as_bytes_mut();
		let url_pointer = url.as_mut_ptr() as i32;
		let url_len = url.len() as i32;

		let headers_pointer = headers.as_mut_ptr() as i32;
		let headers_len = headers.len() as i32;

		let (body_pointer, body_len) = match body.len() {
			0 => (0, 0),
			body_len => (body.as_mut_ptr() as i32, body_len as i32),
		};

		Ok((
			url_pointer,
			url_len,
			method as u8,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
		))
	}
}

#[inline(always)]
fn parse_fileparts_params(
	url: &mut str,
	method: RequestMethod,
	headers: &mut Vec<u8>,
	body: &mut [u8],
	fileparts: &mut Vec<u8>,
) -> Result<(i32, i32, u8, i32, i32, i32, i32, i32, i32), String> {
	let (fileparts_pointer, fileparts_len) = match fileparts.len() {
		0 => (0, 0),
		_ => {
			(fileparts.as_mut_ptr() as i32, fileparts.len() as i32)
		}
	};

	match parse_params(url, method, headers, body) {
		Ok((
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
		)) => Ok((
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
			fileparts_pointer,
			fileparts_len,
		)),
		Err(e) => Err(e),
	}
}

pub fn request(
	mut url: String,
	method: RequestMethod,
	headers: HashMap<&str, String>,
	mut body: Vec<u8>,
) -> Result<(u16, Vec<u8>), String> {
	unsafe {
		let mut headers = match serde_json::to_vec(&headers) {
			Ok(s) => s,
			Err(_) => {
				return Err(String::from("Failed to parse headers"));
			}
		};

		let (url_pointer, url_len, method, headers_pointer, headers_len, body_pointer, body_len) =
			match parse_params(url.as_mut(), method, &mut headers, &mut body) {
				Ok(p) => p,
				Err(e) => return Err(e),
			};
		let result_pointer = send_request(
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
		) as *mut u8;

		let whole = Vec::from_raw_parts(result_pointer, 12, 12);
		let status = i32::from_le_bytes((&whole[8..]).try_into().unwrap());
		let ret_len = i32::from_le_bytes((&whole[4..8]).try_into().unwrap());
		let ret_pointer = i32::from_le_bytes((&whole[..4]).try_into().unwrap());
		let ret = Vec::from_raw_parts(ret_pointer as *mut u8, ret_len as usize, ret_len as usize);

		Ok((status as u16, ret))
	}
}

pub fn async_request(
	mut url: String,
	method: RequestMethod,
	headers: HashMap<&str, String>,
	mut body: Vec<u8>,
) -> Result<(), String> {
	unsafe {
		let mut headers = match serde_json::to_vec(&headers) {
			Ok(s) => s,
			Err(_) => {
				return Err(String::from("Failed to parse headers"));
			}
		};

		let (url_pointer, url_len, method, headers_pointer, headers_len, body_pointer, body_len) =
			match parse_params(url.as_mut(), method, &mut headers, &mut body) {
				Ok(p) => p,
				Err(e) => return Err(e),
			};

		send_async_request(
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
		);

		Ok(())
	}
}

pub fn fileparts_request(
	mut url: String,
	method: RequestMethod,
	headers: HashMap<&str, String>,
	mut body: Vec<u8>,
	fileparts: fileparts::FileParts,
) -> Result<(u16, Vec<u8>), String> {
	unsafe {
		let mut headers = match serde_json::to_vec(&headers) {
			Ok(s) => s,
			Err(_) => {
				return Err(String::from("Failed to parse headers"));
			}
		};

		let mut fileparts = fileparts.to_vec();

		let (
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
			fileparts_pointer,
			fileparts_len,
		) = match parse_fileparts_params(
			url.as_mut(),
			method,
			&mut headers,
			&mut body,
			&mut fileparts,
		) {
			Ok(p) => p,
			Err(e) => return Err(e),
		};
		let result_pointer = send_fileparts_request(
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
			fileparts_pointer,
			fileparts_len,
		) as *mut u8;

		let whole = Vec::from_raw_parts(result_pointer, 12, 12);
		let status = i32::from_le_bytes((&whole[8..]).try_into().unwrap());
		let ret_len = i32::from_le_bytes((&whole[4..8]).try_into().unwrap());
		let ret_pointer = i32::from_le_bytes((&whole[..4]).try_into().unwrap());
		let ret = Vec::from_raw_parts(ret_pointer as *mut u8, ret_len as usize, ret_len as usize);

		Ok((status as u16, ret))
	}
}

pub fn async_fileparts_request(
	mut url: String,
	method: RequestMethod,
	headers: HashMap<&str, String>,
	mut body: Vec<u8>,
	fileparts: fileparts::FileParts,
) -> Result<(), String> {
	unsafe {
		let mut headers = match serde_json::to_vec(&headers) {
			Ok(s) => s,
			Err(_) => {
				return Err(String::from("Failed to parse headers"));
			}
		};

		let mut fileparts = fileparts.to_vec();

		let (
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
			fileparts_pointer,
			fileparts_len,
		) = match parse_fileparts_params(
			url.as_mut(),
			method,
			&mut headers,
			&mut body,
			&mut fileparts,
		) {
			Ok(p) => p,
			Err(e) => return Err(e),
		};

		send_async_fileparts_request(
			url_pointer,
			url_len,
			method,
			headers_pointer,
			headers_len,
			body_pointer,
			body_len,
			fileparts_pointer,
			fileparts_len,
		);

		Ok(())
	}
}
