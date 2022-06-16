use std::{
	env,
	path::Path,
};
use wasmedge_sys::{
    Config,
    Vm,
};
use wasmedge_bindgen_host::Bindgen;


pub struct Wasm {
	bg: Bindgen,
}

impl Wasm {
	pub fn new() -> Wasm {
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
	
		let args: Vec<String> = env::args().collect();
		let wasm_path = Path::new(&args[1]);
		let _ = vm.load_wasm_from_file(wasm_path);
		let _ = vm.validate();
	
		Wasm {bg: Bindgen::new(vm)}
	}

	pub fn exec(&self) {

	}

}

unsafe impl Send for Wasm {}
unsafe impl Sync for Wasm {}