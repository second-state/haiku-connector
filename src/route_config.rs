use std::fs;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum Method {
	#[serde(rename = "GET")]
	Get,
	#[serde(rename = "POST")]
	Post,
	#[serde(rename = "DELETE")]
	Delete
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ContentType {
	#[serde(rename = "text/plain")]
	Plain,
	#[serde(rename = "application/json")]
	Json,
	#[serde(rename = "application/x-www-form-urlencoded")]
	FormUrlencoded, 
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
		toml::from_str(String::from_utf8(fs::read(filepath).unwrap()).unwrap().as_str()).unwrap()
	}
}