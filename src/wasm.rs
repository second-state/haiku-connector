use std::{
	cell::RefCell,
	path::Path,
	time::Duration,
};
use wasmedge_sys::{Config, Vm};
use wasmedge_bindgen_host::{Bindgen, Param};
use reqwest::{Client, ClientBuilder};


const TIMEOUT: u64 = 120;

fn new_http_client() -> Client {
	let cb = ClientBuilder::new().timeout(Duration::from_secs(TIMEOUT));
	return cb.build().unwrap();
}


pub struct Wasm {
	bg: RefCell<Bindgen>,
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
	
		Wasm {bg: RefCell::new(Bindgen::new(vm))}
	}

	pub fn get(&self, func_name: &str, headers: String, queries: String) -> Result<(u16, String, Vec<u8>), String> {
		let params = vec![Param::String(headers), Param::String(queries)];
		match self.bg.borrow_mut().run_wasm(func_name, params) {
			Ok(rv) => {
				if let Ok(mut v) = rv {
					if v.len() == 3 {
						if let Ok(body) = v.pop().unwrap().downcast::<Vec<u8>>() {
							if let Ok(headers) = v.pop().unwrap().downcast::<String>() {
								if let Ok(status) = v.pop().unwrap().downcast::<u16>() {
									return Ok((*status, *headers, *body));
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