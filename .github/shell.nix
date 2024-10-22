{ target }:

let
	rust_overlay = import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");
	pkgs = import <nixpkgs> { overlays = [ rust_overlay ]; };
	rustVersion = "1.82.0";
	rust = pkgs.rust-bin.stable.${rustVersion}.minimal.override {
		targets = [ target ];
	};
in
pkgs.mkShell {
	buildInputs = [ rust ] ++ (with pkgs; [
		pkg-config
		mold
		clang
		patchelf
		gtk4.dev
		gtk4-layer-shell.dev
		libpulseaudio.dev
	]);

	CARGO_BUILD_TARGET = target;
	CARGO_INCREMENTAL = "0";
	CARGO_PROFILE_RELEASE_DEBUG = "none";
	CARGO_PROFILE_RELEASE_LTO = "true";
	CARGO_PROFILE_RELEASE_PANIC = "abort";
	CARGO_PROFILE_RELEASE_STRIP = "symbols";

	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "clang";
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
}
