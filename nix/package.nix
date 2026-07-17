{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "greeter";
  version = "0.1.0";

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  meta = {
    description = "0xc000022070's greeter - a ctOS-flavored ratatui frontend for greetd";
    mainProgram = "greeter";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}
