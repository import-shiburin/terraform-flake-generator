{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "tfg";
  version = "0.1.0";
  src = ./.;
  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "hcl-edit-0.9.4" = "sha256-rsbQsKKr5t18t8QpSENDmgrKxo1sUn/u8zD5DhPbKIY=";
    };
  };
}
