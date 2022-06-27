use std::{
	convert::From,
	sync::{Arc, Mutex},
	path::Path,
	time::Duration, borrow::BorrowMut, str::FromStr,
};
use hyper::HeaderMap;
use serde_json::value::Value;
use wasmedge_sys::*;
use wasmedge_types::ValType;
use wasmedge_bindgen_host::{Bindgen, Param};
use reqwest::{Method, header::{HeaderName, HeaderValue}, blocking::ClientBuilder};

use wasmhaiku_host::RequestMethod;

const TIMEOUT: u64 = 120;

enum WasmEdgeResultCode {
	// SUCCESS = 0, // Success result is always returned with body, so this value is not needed
	TERMINATE = 1,
	FAIL = 2,
}

pub struct Wasm {
	bg: Arc<Mutex<Bindgen>>,
}

impl Clone for Wasm {
	fn clone(&self) -> Self {
		Wasm {
			bg: self.bg.clone(),
		}
	}
}

impl Wasm {
	pub fn new(filepath: String) -> Wasm {
		let mut config = Config::create().unwrap();
		config.wasi(true);
	
		let mut vm = Vm::create(Some(config), None).unwrap();
	
		// get default wasi module
		let mut wasi_module = vm.wasi_module_mut().unwrap();
		// init the default wasi module
		wasi_module.init_wasi(
			Some(vec![]),
			Some(vec![]),
			Some(vec![]),
		);
	
		let wasm_path = Path::new(&filepath);
		let _ = vm.load_wasm_from_file(wasm_path);
		let _ = vm.validate();

		let this = Wasm {bg: Arc::new(Mutex::new(Bindgen::new(vm)))};

		// Register the host function 'send_request'
		let mut imp_obj = ImportModule::create("haiku-connector").unwrap();
		let func_ty = FuncType::create(vec![ValType::I32; 7], vec![ValType::I32; 1]).expect("fail to create a FuncType");
		let boxed_fn = Box::new(this.clone().send_request());
		let func = Function::create(&func_ty, boxed_fn, 0).expect("fail to create a Function instance");
		imp_obj.add_func("send_request", func);

		_ = this.bg.lock().unwrap().borrow_mut().vm().register_wasm_from_import(ImportObject::Import(imp_obj));

		this
	}

	pub fn init(&self) {
		let mut bg = self.bg.lock().unwrap();
		bg.instantiate();
		_ = bg.run_wasm("init", vec![]);
	}

	fn send_request(self) -> impl Fn(Vec<WasmValue>) -> Result<Vec<WasmValue>, u8> {
		let _self = self.clone();
		move |inputs: Vec<WasmValue>| -> Result<Vec<WasmValue>, u8> {
			let mut bg = _self.bg.lock().unwrap();
			let mut mbg = bg.borrow_mut().clone();
			drop(bg);
			let mut memory = mbg.vm().active_module().unwrap().get_memory("memory").unwrap();

			let url = match memory.get_data(inputs[0].to_i32() as u32, inputs[1].to_i32() as u32) {
				Ok(d) => d,
				Err(_) => {
					return Err(WasmEdgeResultCode::TERMINATE as u8);
				}
			};
			let url = match String::from_utf8(url) {
				Ok(s) => s,
				Err(_) => {
					return Err(WasmEdgeResultCode::TERMINATE as u8);
				}
			};

			let method: RequestMethod = (inputs[2].to_i32() as u8).into();
			let method = match Method::from_str(method.to_string().as_str()) {
				Ok(m) => m,
				Err(_) => {
					return Err(WasmEdgeResultCode::TERMINATE as u8);
				}
			};

			let headers = match inputs[3].to_i32() as u32 {
				0 => HeaderMap::new(),
				headers_pointer => {
					let headers = match memory.get_data(headers_pointer, inputs[4].to_i32() as u32) {
						Ok(d) => d,
						Err(_) => {
							return Err(WasmEdgeResultCode::TERMINATE as u8);
						}
					};
					let headers = match String::from_utf8(headers) {
						Ok(s) => s,
						Err(_) => {
							return Err(WasmEdgeResultCode::TERMINATE as u8);
						}
					};
					let headers: Value = match serde_json::from_str(headers.as_str()) {
						Ok(j) => j,
						Err(_) => {
							return Err(WasmEdgeResultCode::TERMINATE as u8);
						}
					};
					let headers = match headers {
						Value::Object(m) => m,
						_ => {
							return Err(WasmEdgeResultCode::TERMINATE as u8);
						}
					};
					let mut header_map = HeaderMap::new();
					for (k, v) in headers.into_iter() {
						if let Ok(hn) = HeaderName::from_str(k.as_str()) {
							if let Ok(hv) = HeaderValue::from_str(v.as_str().unwrap_or_default()) {
								header_map.insert(hn, hv);
							}
						}
					}
					header_map
				}
			};

			let body = match inputs[5].to_i32() as u32 {
				0 => vec![],
				body_pointer => {
					let body = match memory.get_data(body_pointer, inputs[6].to_i32() as u32) {
						Ok(d) => d,
						Err(_) => {
							return Err(WasmEdgeResultCode::TERMINATE as u8);
						}
					};
					body
				}
			};

			match do_request(url, method, headers, body) {
				Ok(ret_body) => {
					let ret_len = ret_body.len() as i32;
					match mbg.vm().run_function("allocate", vec![WasmValue::from_i32(ret_len + 4)]) {
						Ok(rv) => {
							match memory.set_data(ret_body, rv[0].to_i32() as u32 + 4) {
								Ok(()) => {
									match memory.set_data(ret_len.to_le_bytes(), rv[0].to_i32() as u32) {
										Ok(()) => {
											return Ok(vec![WasmValue::from_i32(rv[0].to_i32())]);
										}
										Err(_) => {
											return Err(WasmEdgeResultCode::TERMINATE as u8);
										}
									}
								}
								Err(_) => {
									return Err(WasmEdgeResultCode::TERMINATE as u8);
								}
							}
						}
						Err(_) => {
							return Err(WasmEdgeResultCode::TERMINATE as u8);
						}
					}
				}
				Err(_) => {
					return Err(WasmEdgeResultCode::FAIL as u8);
				}
			}
		}
	}

	pub fn execute(&self, func_name: &str, headers: String, queries: String, body: Vec<u8>) -> Result<(u16, String, Vec<u8>), String> {
		let params = vec![Param::String(headers), Param::String(queries), Param::VecU8(body)];
		let mut bg = self.bg.lock().unwrap();
		let mut mbg = bg.borrow_mut().clone();
		drop(bg);
		match mbg.run_wasm(func_name, params) {
			Ok(rv) => {
				if let Ok(mut v) = rv {
					if v.len() == 3 {
						if let Ok(ret_body) = v.pop().unwrap().downcast::<Vec<u8>>() {
							if let Ok(ret_headers) = v.pop().unwrap().downcast::<String>() {
								if let Ok(ret_status) = v.pop().unwrap().downcast::<u16>() {
									return Ok((*ret_status, *ret_headers, *ret_body));
								}
							}
						}
					}
				}
				Err(String::from("Invalid return values"))
			}
			Err(e) => {
				Err(format!("{:?}", e))
			}
		}
	}

}

unsafe impl Send for Wasm {}
unsafe impl Sync for Wasm {}

fn do_request(url: String, method: Method, headers: HeaderMap, body: Vec<u8>) -> Result<Vec<u8>, String> {
	tokio::task::block_in_place(move || {
		let c = ClientBuilder::new().timeout(Duration::from_secs(TIMEOUT)).build().unwrap();
		match c.request(method, url).headers(headers).body(body).send() {
			Ok(r) => {
				match r.bytes() {
					Ok(b) => {
						Ok(b.as_ref().to_vec())
					}
					Err(e) => {
						Err(format!("{:?}", e))
					}
				}
			}
			Err(e) => {
				Err(format!("{:?}", e))
			}
		}
	})
}