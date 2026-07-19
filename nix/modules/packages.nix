{ inputs, ... }:
{
  perSystem =
    {
      pkgs,
      lib,
      system,
      ...
    }:
    let
      mkMav = import ../toolchain.nix { inherit inputs; };
      mav-editor = mkMav pkgs;
    in
    {
      packages = {
        default = mav-editor;
        debug = mav-editor.override { profile = "dev"; };
      };
    }
    // lib.optionalAttrs (lib.hasSuffix "linux" system) {
      checks = import ../tests/sandboxing { inherit pkgs inputs; };
    };
}
