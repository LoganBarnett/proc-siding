# Crane-based derivation for the proc-siding-web binary.
# Called from flake.nix with: import ./web.nix { inherit craneLib commonArgs; }
{ craneLib, commonArgs }:
craneLib.buildPackage (commonArgs // {
  pname = "proc-siding-web";
  cargoExtraArgs = "-p proc-siding-web";
})
