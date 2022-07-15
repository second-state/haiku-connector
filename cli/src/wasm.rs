use hyper::HeaderMap;
use reqwest::{
	blocking::{multipart, ClientBuilder},
	header::{HeaderName, HeaderValue},
	Method,
};
use serde_json::value::Value;
use std::{
	borrow::BorrowMut,
	convert::From,
	path::Path,
	str::FromStr,
	sync::{Arc, Mutex},
	time::Duration,
};
use wasmedge_bindgen_host::{Bindgen, Param};
use wasmedge_sys::*;
use wasmedge_types::ValType;

use wasmhaiku_glue::{fileparts::FileParts, RequestMethod};

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
		wasi_module.init_wasi(Some(vec![]), Some(vec![]), Some(vec![]));

		let wasm_path = Path::new(&filepath);
		let _ = vm.load_wasm_from_file(wasm_path);
		let _ = vm.validate();

		let this = Wasm {
			bg: Arc::new(Mutex::new(Bindgen::new(vm))),
		};

		{
			let mut mut_guard = this.bg.lock().unwrap();
			let vm = mut_guard.borrow_mut().vm();

			// Register the host function 'send_request'
			let mut imp_obj = ImportModule::create("haiku-connector").unwrap();

			let func_ty = FuncType::create(vec![ValType::I32; 7], vec![ValType::I32; 1])
				.expect("fail to create a FuncType");
			let boxed_fn = Box::new(this.clone().send_request());
			let func = Function::create(&func_ty, boxed_fn, 0)
				.expect("fail to create a Function instance");
			imp_obj.add_func("send_request", func);

			// Register the host function 'send_async_request'
			let func_ty =
				FuncType::create(vec![ValType::I32; 7], vec![]).expect("fail to create a FuncType");
			let boxed_fn = Box::new(this.clone().send_async_request());
			let func = Function::create(&func_ty, boxed_fn, 0)
				.expect("fail to create a Function instance");
			imp_obj.add_func("send_async_request", func);

			// Register the host function 'send_fileparts_request'
			let func_ty = FuncType::create(vec![ValType::I32; 9], vec![ValType::I32; 1])
				.expect("fail to create a FuncType");
			let boxed_fn = Box::new(this.clone().send_fileparts_request());
			let func = Function::create(&func_ty, boxed_fn, 0)
				.expect("fail to create a Function instance");
			imp_obj.add_func("send_fileparts_request", func);

			// Register the host function 'send_async_fileparts_request'
			let func_ty =
				FuncType::create(vec![ValType::I32; 9], vec![]).expect("fail to create a FuncType");
			let boxed_fn = Box::new(this.clone().send_async_fileparts_request());
			let func = Function::create(&func_ty, boxed_fn, 0)
				.expect("fail to create a Function instance");
			imp_obj.add_func("send_async_fileparts_request", func);

			vm.register_wasm_from_import(ImportObject::Import(imp_obj))
				.unwrap();
		}

		this
	}

	pub fn init(&self) {
		let mut bg = self.bg.lock().unwrap();
		bg.instantiate();
		_ = bg.run_wasm("init", vec![]);
	}

	fn parse_params(
		memory: &Memory,
		inputs: Vec<WasmValue>,
	) -> Result<(String, Method, HeaderMap, Vec<u8>), u8> {
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

		Ok((url, method, headers, body))
	}

	fn parse_fileparts_params(
		memory: &Memory,
		inputs: Vec<WasmValue>,
	) -> Result<(String, Method, HeaderMap, Vec<u8>, Vec<u8>), u8> {
		let fileparts = match inputs[7].to_i32() as u32 {
			0 => vec![],
			fileparts_pointer => {
				let fileparts = match memory.get_data(fileparts_pointer, inputs[8].to_i32() as u32)
				{
					Ok(d) => d,
					Err(_) => {
						return Err(WasmEdgeResultCode::TERMINATE as u8);
					}
				};
				fileparts
			}
		};

		match Wasm::parse_params(memory, inputs) {
			Ok((url, method, headers, body)) => Ok((url, method, headers, body, fileparts)),
			Err(e) => Err(e),
		}
	}

	fn set_wasm_memory(data: Vec<u8>, memory: &mut Memory, vm: &Vm) -> Result<i32, u8> {
		let len = data.len();
		match vm.run_function("allocate", vec![WasmValue::from_i32(len as i32)]) {
			Ok(rv) => match memory.set_data(data, rv[0].to_i32() as u32) {
				Ok(_) => Ok(rv[0].to_i32()),
				Err(_) => Err(WasmEdgeResultCode::TERMINATE as u8),
			},
			Err(_) => Err(WasmEdgeResultCode::TERMINATE as u8),
		}
	}

	fn settle_result(
		status: u16,
		ret_body: Vec<u8>,
		memory: &mut Memory,
		vm: &Vm,
	) -> Result<Vec<WasmValue>, u8> {
		let body_len: [u8; 4] = (ret_body.len() as i32).to_le_bytes().try_into().unwrap();
		let body_pointer = match Wasm::set_wasm_memory(ret_body, memory, vm) {
			Ok(p) => p.to_le_bytes().try_into().unwrap(),
			Err(e) => return Err(e),
		};

		let status = (status as i32).to_le_bytes().try_into().unwrap();
		let whole = [body_pointer, body_len, status].concat();
		let whole_pointer = match Wasm::set_wasm_memory(whole, memory, vm) {
			Ok(p) => p,
			Err(e) => return Err(e),
		};
		Ok(vec![WasmValue::from_i32(whole_pointer)])
	}

	fn send_request(self) -> impl Fn(Vec<WasmValue>) -> Result<Vec<WasmValue>, u8> {
		move |inputs: Vec<WasmValue>| -> Result<Vec<WasmValue>, u8> {
			let mut bg = self.bg.lock().unwrap();
			let mut mbg = bg.borrow_mut().clone();
			drop(bg);
			let mut memory = mbg
				.vm()
				.active_module()
				.unwrap()
				.get_memory("memory")
				.unwrap();

			let (url, method, headers, body) = match Wasm::parse_params(&memory, inputs) {
				Ok(p) => p,
				Err(e) => return Err(e),
			};

			match Wasm::do_request(url, method, headers, body) {
				Ok((status, ret_body)) => {
					let vm = mbg.vm();
					Wasm::settle_result(status, ret_body, &mut memory, vm)
				}

				Err(_) => Err(WasmEdgeResultCode::FAIL as u8),
			}
		}
	}

	fn send_async_request(self) -> impl Fn(Vec<WasmValue>) -> Result<Vec<WasmValue>, u8> {
		move |inputs: Vec<WasmValue>| -> Result<Vec<WasmValue>, u8> {
			let mut bg = self.bg.lock().unwrap();
			let mut mbg = bg.borrow_mut().clone();
			drop(bg);
			let memory = mbg
				.vm()
				.active_module()
				.unwrap()
				.get_memory("memory")
				.unwrap();

			let (url, method, headers, body) = match Wasm::parse_params(&memory, inputs) {
				Ok(p) => p,
				Err(e) => return Err(e),
			};

			tokio::spawn(async move {
				let _ = Wasm::do_request(url, method, headers, body);
			});

			Ok(vec![])
		}
	}

	fn send_fileparts_request(self) -> impl Fn(Vec<WasmValue>) -> Result<Vec<WasmValue>, u8> {
		move |inputs: Vec<WasmValue>| -> Result<Vec<WasmValue>, u8> {
			let mut bg = self.bg.lock().unwrap();
			let mut mbg = bg.borrow_mut().clone();
			drop(bg);
			let mut memory = mbg
				.vm()
				.active_module()
				.unwrap()
				.get_memory("memory")
				.unwrap();

			let (url, method, headers, body, fileparts) =
				match Wasm::parse_fileparts_params(&memory, inputs) {
					Ok(p) => p,
					Err(e) => return Err(e),
				};

			match Wasm::do_fileparts_request(url, method, headers, body, fileparts) {
				Ok((status, ret_body)) => {
					let vm = mbg.vm();
					Wasm::settle_result(status, ret_body, &mut memory, vm)
				}

				Err(_) => Err(WasmEdgeResultCode::FAIL as u8),
			}
		}
	}

	fn send_async_fileparts_request(self) -> impl Fn(Vec<WasmValue>) -> Result<Vec<WasmValue>, u8> {
		move |inputs: Vec<WasmValue>| -> Result<Vec<WasmValue>, u8> {
			let mut bg = self.bg.lock().unwrap();
			let mut mbg = bg.borrow_mut().clone();
			drop(bg);
			let memory = mbg
				.vm()
				.active_module()
				.unwrap()
				.get_memory("memory")
				.unwrap();

			let (url, method, headers, body, fileparts) =
				match Wasm::parse_fileparts_params(&memory, inputs) {
					Ok(p) => p,
					Err(e) => return Err(e),
				};

			tokio::spawn(async move {
				let _ = Wasm::do_fileparts_request(url, method, headers, body, fileparts);
			});

			Ok(vec![])
		}
	}

	pub fn execute(
		&self,
		func_name: &str,
		headers: &str,
		queries: &str,
		body: &Vec<u8>,
	) -> Result<(u16, String, Vec<u8>), String> {
		let params = vec![
			Param::String(headers),
			Param::String(queries),
			Param::VecU8(body),
		];
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
			Err(e) => Err(format!("{:?}", e)),
		}
	}

	pub fn execute_fileparts(
		&self,
		func_name: &str,
		headers: &str,
		queries: &str,
		body: &Vec<u8>,
		fileparts: &Vec<u8>,
	) -> Result<(u16, String, Vec<u8>), String> {
		let params = vec![
			Param::String(headers),
			Param::String(queries),
			Param::VecU8(body),
			Param::VecU8(fileparts),
		];
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
			Err(e) => Err(format!("{:?}", e)),
		}
	}

	fn do_request(
		url: String,
		method: Method,
		headers: HeaderMap,
		body: Vec<u8>,
	) -> Result<(u16, Vec<u8>), String> {
		tokio::task::block_in_place(move || {
			let c = ClientBuilder::new()
				.timeout(Duration::from_secs(TIMEOUT))
				.build()
				.unwrap();
			match c.request(method, url).headers(headers).body(body).send() {
				Ok(r) => {
					let status = r.status().as_u16();
					match r.bytes() {
						Ok(b) => Ok((status, b.as_ref().to_vec())),
						Err(e) => Err(format!("{:?}", e)),
					}
				}
				Err(e) => Err(format!("{:?}", e)),
			}
		})
	}

	fn do_fileparts_request(
		url: String,
		method: Method,
		headers: HeaderMap,
		body: Vec<u8>,
		fileparts: Vec<u8>,
	) -> Result<(u16, Vec<u8>), String> {
		tokio::task::block_in_place(move || {
			let c = ClientBuilder::new()
				.timeout(Duration::from_secs(TIMEOUT))
				.build()
				.unwrap();

			let mut request = multipart::Form::new();
			match serde_json::from_slice(&body) {
				Ok(Value::Object(b)) => {
					request = b.into_iter().fold(request, |accum, (k, v)| {
						if v.is_string() {
							return accum.text(k, v.as_str().unwrap().to_string());
						}
						accum
					});
				}
				_ => (),
			}

			let fps: FileParts = fileparts.into();
			for f in fps.inner.into_iter() {
				if let Ok(part) = multipart::Part::bytes(f.bytes)
					.file_name(f.file_name)
					.mime_str(&f.mime_str)
				{
					request = request.part("file", part);
				}
			}
			match c
				.request(method, url)
				.headers(headers)
				.multipart(request)
				.send()
			{
				Ok(r) => {
					let status = r.status().as_u16();
					match r.bytes() {
						Ok(b) => Ok((status, b.as_ref().to_vec())),
						Err(e) => Err(format!("{:?}", e)),
					}
				}
				Err(e) => Err(format!("{:?}", e)),
			}
		})
	}
}

unsafe impl Send for Wasm {}
unsafe impl Sync for Wasm {}
