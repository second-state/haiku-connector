mod wasm;
mod route_config;
mod initial;

use std::{
    env,
    collections::HashMap,
    net::SocketAddr,
};

use lazy_static::lazy_static;
use axum::{
	Router,
    body::Body,
	routing::any,
	extract::{Path, Query},
	response::{IntoResponse},
	http::{Request, header::{HeaderName, HeaderValue, HeaderMap}, StatusCode},
};
use hyper::body::to_bytes;

use initial::Initial;

lazy_static! {
    static ref INIT: Initial = Initial::new();
}

async fn handler(
		headers: HeaderMap,
		Path(path): Path<String>,
		Query(queries): Query<HashMap<String, String>>,
		mut req: Request<Body>) -> impl IntoResponse {
	let method = format!("\"{}\"", req.method().as_ref());
	let body = match to_bytes(req.body_mut()).await {
		Ok(bytes) => {
			bytes.as_ref().to_vec()
		}
		Err(e) => {
			return Err((StatusCode::BAD_REQUEST, format!("{:?}", e).as_bytes().to_vec()));
		}
	};
	let headers = format!("{:?}", headers);
	let queries = serde_json::to_string(&queries).unwrap();

	for c in INIT.config.route.iter() {
		if path.eq(c.path.as_str()) {
			if method.eq(serde_json::to_string(&c.method).unwrap().as_str()) {
				match INIT.wasm.execute(c.func_name.as_str(), headers, queries, body) {
					Ok((ret_status, ret_headers, ret_body)) => {
						let ret_headers: HashMap<String, String> = match serde_json::from_str(ret_headers.as_str()) {
							Ok(h) => h,
							Err(e) => {
								return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid response headers. {:?}", e).as_bytes().to_vec()));
							}
						};
						let mut ret_header_map = HeaderMap::new();
						for (k, v) in ret_headers.iter() {
							if let Ok(header_name) = HeaderName::from_bytes(k.as_bytes()) {
								if let Ok(header_value) = HeaderValue::from_str(v) {
									ret_header_map.insert(header_name, header_value);
								}
							}
						}
						return Ok((StatusCode::from_u16(ret_status).unwrap(), ret_header_map, ret_body));
					}
					Err(e) => {
						return Err((StatusCode::INTERNAL_SERVER_ERROR, e.as_bytes().to_vec()));
					}
				}
			}
			// No else here because we should support one path with two or more methods.
			// e.g. we should support both GET /example and POST /example
		}
	}
    Err((StatusCode::NOT_FOUND, "Not found".as_bytes().to_vec()))
}

#[tokio::main]
async fn main() {
	INIT.wasm.init();

	let app = Router::new()
        .route("/*route", any(handler));

	let port = env::var("PORT").unwrap_or_else(|_| "9000".to_string());
	let port = port.parse::<u16>().unwrap();
	let addr = SocketAddr::from(([127, 0, 0, 1], port));

	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}