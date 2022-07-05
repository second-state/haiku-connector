use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Method {
	#[serde(rename = "DELETE")]
	Delete = 2,
	#[serde(rename = "GET")]
	Get = 4,
	#[serde(rename = "HEAD")]
	Head = 8,
	#[serde(rename = "OPTIONS")]
	Options = 16,
	#[serde(rename = "PATCH")]
	Patch = 32,
	#[serde(rename = "POST")]
	Post = 64,
	#[serde(rename = "PUT")]
	Put = 128,
	#[serde(rename = "TRACE")]
	Trace = 256,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ContentType {
	#[serde(rename = "text/plain")]
	Plain,
	#[serde(rename = "application/json")]
	Json,
	#[serde(rename = "application/x-www-form-urlencoded")]
	FormUrlencoded,
	#[serde(rename = "multipart/form-data")]
	Multipart,
}

#[derive(Debug, Deserialize)]
pub struct Route {
	pub func_name: String,
	pub path: String,
	pub method: Method,
	pub content_type: Option<ContentType>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
	pub route: Vec<Route>,
}

impl Config {
	pub fn new(filepath: String) -> Config {
		toml::from_str(
			String::from_utf8(fs::read(filepath).unwrap())
				.unwrap()
				.as_str(),
		)
		.unwrap()
	}
}
