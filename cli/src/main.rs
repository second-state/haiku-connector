mod initial;
mod route_config;
mod wasm;

use std::{collections::HashMap, env, future::Future, net::SocketAddr, pin::Pin};

use axum::{
	body::Bytes,
	extract::{ContentLengthLimit, Multipart, Query},
	handler::Handler,
	http::{
		header::{HeaderMap, HeaderName, HeaderValue},
		StatusCode,
	},
	routing::{self, MethodFilter},
	Router,
};
use lazy_static::lazy_static;

use wasmhaiku_glue::fileparts::{FilePart, FileParts};

use initial::Initial;

lazy_static! {
	static ref INIT: Initial = Initial::new();
}

fn settle_resp(
	ret_status: u16,
	ret_headers: String,
	ret_body: Vec<u8>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>), (StatusCode, Vec<u8>)> {
	let mut ret_header_map = HeaderMap::new();

	if ret_headers.len() > 0 {
		let ret_headers: HashMap<String, String> = match serde_json::from_str(ret_headers.as_str())
		{
			Ok(h) => h,
			Err(e) => {
				return Err((
					StatusCode::INTERNAL_SERVER_ERROR,
					format!("Invalid response headers. {:?}", e)
						.as_bytes()
						.to_vec(),
				));
			}
		};
		for (k, v) in ret_headers.iter() {
			if let Ok(header_name) = HeaderName::from_bytes(k.as_bytes()) {
				if let Ok(header_value) = HeaderValue::from_str(v) {
					ret_header_map.insert(header_name, header_value);
				}
			}
		}
	}
	return Ok((
		StatusCode::from_u16(ret_status).unwrap(),
		ret_header_map,
		ret_body,
	));
}

fn handler(
	func_name: String,
	async_func_name: Option<String>,
) -> impl Handler<(HeaderMap, Query<HashMap<String, String>>, Bytes)> {
	return |headers: HeaderMap,
	        Query(queries): Query<HashMap<String, String>>,
	        bytes: Bytes|
	 -> Pin<
		Box<
			dyn Future<Output = Result<(StatusCode, HeaderMap, Vec<u8>), (StatusCode, Vec<u8>)>>
				+ Send,
		>,
	> {
		return Box::pin(async move {
			let body = bytes.to_vec();
			let headers = format!("{:?}", headers);
			let queries = serde_json::to_string(&queries).unwrap();
			match INIT.wasm.execute(
				func_name.as_str(),
				headers.as_str(),
				queries.as_str(),
				&body,
			) {
				Ok((ret_status, ret_headers, ret_body)) => {
					if async_func_name.is_some() && ret_status == 100 {
						tokio::spawn(async move {
							let _ = INIT.wasm.execute(
								async_func_name.unwrap().as_str(),
								headers.as_str(),
								queries.as_str(),
								&body,
							);
						});
						// return 200 if the async func is called
						settle_resp(200, ret_headers, ret_body)
					} else {
						settle_resp(ret_status, ret_headers, ret_body)
					}
				}
				Err(e) => {
					return Err((StatusCode::INTERNAL_SERVER_ERROR, e.as_bytes().to_vec()));
				}
			}
		});
	};
}

fn multipart_handler(
	func_name: String,
	async_func_name: Option<String>,
) -> impl Handler<(
	HeaderMap,
	Query<HashMap<String, String>>,
	ContentLengthLimit<Multipart, { 10 * 1024 * 1024 }>,
)> {
	return |headers: HeaderMap,
	        Query(queries): Query<HashMap<String, String>>,
	        ContentLengthLimit(mut multipart): ContentLengthLimit<
		Multipart,
		{
			10 * 1024 * 1024 /* 10mb */
		},
	>|
	 -> Pin<
		Box<
			dyn Future<Output = Result<(StatusCode, HeaderMap, Vec<u8>), (StatusCode, Vec<u8>)>>
				+ Send,
		>,
	> {
		return Box::pin(async move {
			let mut fileparts: Vec<FilePart> = vec![];
			let mut body: HashMap<String, String> = HashMap::new();

			while let Some(field) = multipart.next_field().await.unwrap() {
				match field.name() {
					Some(name) => {
						let name = name.to_string();
						if let Some(file_name) = field.file_name() {
							let file_name = file_name.to_string();
							if let Some(mime_str) = field.content_type() {
								let mime_str = mime_str.to_string();
								match field.bytes().await {
									Ok(bytes) => {
										fileparts.push(FilePart {
											file_name,
											mime_str,
											bytes: bytes.to_vec(),
										});
										continue;
									}
									Err(_) => {
										return Err((
											StatusCode::BAD_REQUEST,
											b"field without name".to_vec(),
										));
									}
								}
							}
						}

						// no file_name or content_type
						match field.text().await {
							Ok(text) => {
								body.insert(name, text);
							}
							Err(_) => {
								return Err((
									StatusCode::BAD_REQUEST,
									b"field without name".to_vec(),
								));
							}
						}
					}
					None => {
						return Err((StatusCode::BAD_REQUEST, b"field without name".to_vec()));
					}
				}
			}

			let body = serde_json::to_vec(&body).unwrap();
			let fileparts = FileParts { inner: fileparts }.to_vec();
			let headers = format!("{:?}", headers);
			let queries = serde_json::to_string(&queries).unwrap();

			match INIT.wasm.execute_fileparts(
				func_name.as_str(),
				headers.as_str(),
				queries.as_str(),
				&body,
				&fileparts,
			) {
				Ok((ret_status, ret_headers, ret_body)) => {
					if async_func_name.is_some() && ret_status == 100 {
						tokio::spawn(async move {
							let _ = INIT.wasm.execute_fileparts(
								async_func_name.unwrap().as_str(),
								headers.as_str(),
								queries.as_str(),
								&body,
								&fileparts,
							);
						});
						// return 200 if the async func is called
						settle_resp(200, ret_headers, ret_body)
					} else {
						settle_resp(ret_status, ret_headers, ret_body)
					}
				}
				Err(e) => {
					return Err((StatusCode::INTERNAL_SERVER_ERROR, e.as_bytes().to_vec()));
				}
			}
		});
	};
}

#[tokio::main]
async fn main() {
	INIT.wasm.init();

	let mut app = Router::new();

	for c in INIT.config.route.iter() {
		app = match c.content_type {
			Some(route_config::ContentType::Multipart) => app.route(
				c.path.as_str(),
				routing::on(
					MethodFilter::from_bits(c.method as u16).unwrap(),
					multipart_handler(c.func_name.to_string(), c.async_func_name.clone()),
				),
			),
			_ => app.route(
				c.path.as_str(),
				routing::on(
					MethodFilter::from_bits(c.method as u16).unwrap(),
					handler(c.func_name.to_string(), c.async_func_name.clone()),
				),
			),
		}
	}

	let port = env::var("PORT").unwrap_or_else(|_| "9000".to_string());
	let port = port.parse::<u16>().unwrap();
	let addr = SocketAddr::from(([127, 0, 0, 1], port));

	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}
