{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "runapp";
  version = (lib.importTOML ./Cargo.toml).package.version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      (lib.fileset.fileFilter (file: file.hasExt "rs") ./src)
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Sets up environment for running the application";
    homepage = "https://github.com/HNIKT-Tjenesteutvikling-Systemutvikling/kvalreg-runapp";
    license = lib.licenses.gpl3Only;
    mainProgram = "runapp";
  };
}
