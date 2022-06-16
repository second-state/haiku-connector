mod wasm;

use std::{
    env,
    collections::HashMap,
time::Duration,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use lazy_static::lazy_static;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use reqwest::{Client, ClientBuilder, multipart};
use axum::{
	Router,
    body::Body,
	routing::any,
	extract::{Path, Query, Json},
	response::{IntoResponse},
	http::{Request, StatusCode},
	extract::{ContentLengthLimit, Multipart},
};

use wasm::Wasm;

lazy_static! {
	static ref WASM_EXECUTOR: Arc<Mutex<Wasm>> = Arc::new(Mutex::new(Wasm::new())); 
}

const TIMEOUT: u64 = 120;

fn new_http_client() -> Client {
	let cb = ClientBuilder::new().timeout(Duration::from_secs(TIMEOUT));
	return cb.build().unwrap();
}

async fn handler(Path(path): Path<HashMap<String, String>>, Query(_): Query<HashMap<String, String>>, req: Request<Body>) -> impl IntoResponse {
    println!("{:?}", req);
    let route = path.get("route").unwrap();
    match WASM_EXECUTOR.lock() {
        Ok(executor) => {
            executor.exec();
        }
        Err(_) => ()
    }
    (StatusCode::OK, route.to_owned())
}

#[tokio::main]
async fn main() {
	let app = Router::new()
        .route("/:route", any(handler));

	let port = env::var("PORT").unwrap_or_else(|_| "9000".to_string());
	let port = port.parse::<u16>().unwrap();
	let addr = SocketAddr::from(([127, 0, 0, 1], port));

	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}