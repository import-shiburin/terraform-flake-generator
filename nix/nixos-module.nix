flake:
{ config, lib, pkgs, ... }:
let
  cfg = config.programs.tfg;
  wrappedPackage = pkgs.symlinkJoin {
    name = "tfg-wrapped";
    paths = [ cfg.package ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      wrapProgram $out/bin/tfg \
        --run 'export GITHUB_TOKEN="$(cat "${cfg.githubTokenFile}")"'
    '';
  };
  finalPackage = if cfg.githubTokenFile != null then wrappedPackage else cfg.package;
in
{
  options.programs.tfg = {
    enable = lib.mkEnableOption "tfg";
    package = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The tfg package to use.";
    };
    githubTokenFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Path to a file containing a GitHub token. If set, the binary will be wrapped with GITHUB_TOKEN environment variable read from this file at runtime.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ finalPackage ];
  };
}
