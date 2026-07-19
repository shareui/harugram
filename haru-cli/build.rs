fn main() {
	cc::Build::new().file("src/cxx/bithash/bithash.c").include("src/cxx/bithash").compile("bithash");

	println!("cargo:rerun-if-changed=src/cxx/bithash/bithash.c");
	println!("cargo:rerun-if-changed=src/cxx/bithash/bithash.h");
}
