flake:
{ config, lib, pkgs, ... }:
let
  cfg = config.programs.tfg;
in
{
  options.programs.tfg = {
    enable = lib.mkEnableOption "tfg";
    package = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The tfg package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
  };
}
