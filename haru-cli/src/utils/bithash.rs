// unused for now, reserved for the build cache
#![allow(dead_code)]

use std::ffi::CString;
use std::os::raw::{c_int, c_void};

pub const BUF_SIZE: usize = 32;
pub const SEED_DEFAULT: u64 = 0;

#[repr(C)]
pub struct BitHashState {
	pub s0: u64,
	pub s1: u64,
	pub s2: u64,
	pub s3: u64,
	pub total_len: u64,
	pub buf: [u8; BUF_SIZE],
	pub buf_len: u32,
}

unsafe extern "C" {
	fn bitHash_oneshot(data: *const c_void, len: usize, seed: u64) -> u64;
	fn bitHash_init(state: *mut BitHashState, seed: u64);
	fn bitHash_update(state: *mut BitHashState, data: *const c_void, len: usize);
	fn bitHash_finish(state: *mut BitHashState) -> u64;
	fn bitHash_file(path: *const i8, io_buf: *mut c_void, io_buf_len: usize, seed: u64, out_hash: *mut u64) -> c_int;
	fn bitHash_files_equal(path_a: *const i8, path_b: *const i8, io_buf: *mut c_void, io_buf_len: usize) -> c_int;
	fn bitHash_version() -> *const i8;
}

pub fn oneshot(data: &[u8], seed: u64) -> u64 {
	unsafe { bitHash_oneshot(data.as_ptr().cast(), data.len(), seed) }
}

pub struct Hasher {
	state: BitHashState,
}

impl Hasher {
	pub fn new(seed: u64) -> Self {
		let mut state = BitHashState { s0: 0, s1: 0, s2: 0, s3: 0, total_len: 0, buf: [0; BUF_SIZE], buf_len: 0 };
		unsafe { bitHash_init(&mut state, seed) };
		Self { state }
	}

	pub fn update(&mut self, data: &[u8]) {
		unsafe { bitHash_update(&mut self.state, data.as_ptr().cast(), data.len()) };
	}

	pub fn finish(mut self) -> u64 {
		unsafe { bitHash_finish(&mut self.state) }
	}
}

#[derive(Debug)]
pub enum Error {
	InvalidPath(String),
	Native,
}

pub fn hash_file(path: &str, seed: u64) -> Result<u64, Error> {
	let c_path = CString::new(path).map_err(|_| Error::InvalidPath(path.to_string()))?;
	let mut io_buf = [0u8; 65536];
	let mut out_hash: u64 = 0;

	let rc = unsafe { bitHash_file(c_path.as_ptr(), io_buf.as_mut_ptr().cast(), io_buf.len(), seed, &mut out_hash) };
	if rc != 0 {
		return Err(Error::Native);
	}
	Ok(out_hash)
}

pub fn files_equal(path_a: &str, path_b: &str) -> Result<bool, Error> {
	let c_path_a = CString::new(path_a).map_err(|_| Error::InvalidPath(path_a.to_string()))?;
	let c_path_b = CString::new(path_b).map_err(|_| Error::InvalidPath(path_b.to_string()))?;
	let mut io_buf = [0u8; 65536];

	let rc = unsafe { bitHash_files_equal(c_path_a.as_ptr(), c_path_b.as_ptr(), io_buf.as_mut_ptr().cast(), io_buf.len()) };
	match rc {
		1 => Ok(true),
		0 => Ok(false),
		_ => Err(Error::Native),
	}
}

pub fn version() -> &'static str {
	unsafe {
		let ptr = bitHash_version();
		std::ffi::CStr::from_ptr(ptr).to_str().unwrap_or("unknown")
	}
}
