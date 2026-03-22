{ writers, python3Packages }:
writers.writePython3Bin "mlx-metal-test"
  { libraries = [ python3Packages.mlx ]; flakeIgnore = [ "E265" ]; }
  (builtins.readFile ./mlx-metal-test)
