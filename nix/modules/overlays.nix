{ inputs, ... }:
{
  flake.overlays.default =
    final: _:
    let
      mkMav = import ../toolchain.nix { inherit inputs; };
    in
    {
      mav-editor = mkMav final;
    };
}
