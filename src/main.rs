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

use initial::Initial;

lazy_static! {
    static ref INIT: Initial = Initial::new();
}

async fn handler(
		headers: HeaderMap,
		Path(path): Path<String>,
		Query(queries): Query<HashMap<String, String>>,
		req: Request<Body>) -> impl IntoResponse {
	let method = req.method();
	let headers = format!("{:?}", headers);
	let queries = serde_json::to_string(&queries).unwrap();
	for c in INIT.config.route.iter() {
		if path.eq(c.path.as_str()) {
			if format!("\"{}\"", method.as_str()).eq(serde_json::to_string(&c.method).unwrap().as_str()) {
				match INIT.wasm.get(c.func_name.as_str(), headers, queries) {
					Ok((status, headers, body)) => {
						let headers: HashMap<String, String> = serde_json::from_str(headers.as_str()).unwrap();
						let mut header_map = HeaderMap::new();
						for (k, v) in headers.iter() {
							if let Ok(header_name) = HeaderName::from_bytes(k.as_bytes()) {
								if let Ok(header_value) = HeaderValue::from_str(v) {
									header_map.insert(header_name, header_value);
								}
							}
						}
						return Ok((StatusCode::from_u16(status).unwrap(), header_map, body));
					}
					Err(e) => {
						return Err((StatusCode::INTERNAL_SERVER_ERROR, e.as_bytes().to_vec()));
					}
				}
			}
		}
	}
    Err((StatusCode::NOT_FOUND, "Not found".as_bytes().to_vec()))
}

#[tokio::main]
async fn main() {
    println!("{:?}", INIT.config);
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