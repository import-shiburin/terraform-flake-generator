# tfg

`tfg` generates a `flake.nix` that pins Terraform to a specific version from
nixpkgs. Point it at a directory with `.tf` files and it will read the
`required_version` constraint, search NixOS/nixpkgs for a commit that ships a
matching Terraform, and write (or update) a flake that puts that version in a
dev shell via flake-parts.

## Installation

### Prebuilt binaries

Download a binary for your platform from the
[Releases](https://github.com/import-shiburin/terraform-flake-generator/releases)
page. Each release includes an `.md5` checksum file you can verify with:

```
md5sum -c tfg-<target>.md5
```

Available targets: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-apple-darwin`, `aarch64-apple-darwin`.

### Nix

With Nix (static musl binary):

```
nix build github:import-shiburin/terraform-flake-generator
```

Or with Cargo:

```
cargo build --release
```

The binary lands at `target/release/tfg`.

### NixOS module

```nix
# flake.nix
{
  inputs.tfg.url = "github:import-shiburin/terraform-flake-generator";

  outputs = { nixpkgs, tfg, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        tfg.nixosModules.default
        { programs.tfg.enable = true; }
      ];
    };
  };
}
```

### Home Manager module

```nix
# flake.nix
{
  inputs.tfg.url = "github:import-shiburin/terraform-flake-generator";

  outputs = { tfg, ... }: {
    # inside your home-manager config:
    imports = [ tfg.homeManagerModules.default ];
    programs.tfg.enable = true;
  };
}
```

### GitHub token file (NixOS / Home Manager)

Both modules accept a `githubTokenFile` option. When set, the binary is
wrapped so that `GITHUB_TOKEN` is read from the file at runtime. The value
is a string, so shell variables are expanded at runtime.

```nix
programs.tfg = {
  enable = true;
  githubTokenFile = config.sops.secrets.github-token.path; # sops-nix
  # githubTokenFile = config.age.secrets.github-token.path; # agenix
  # githubTokenFile = "\${XDG_RUNTIME_DIR}/agenix/github-token"; # path with shell variable
};
```

### Overlay

```nix
# flake.nix
{
  inputs.tfg.url = "github:import-shiburin/terraform-flake-generator";

  outputs = { nixpkgs, tfg, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [{
        nixpkgs.overlays = [ tfg.overlays.default ];
        environment.systemPackages = [ pkgs.tfg ];
      }];
    };
  };
}
```

### Without flakes

```
nix-build -E 'with import <nixpkgs> {}; callPackage ./default.nix {}'
```

## Usage

```
tfg [OPTIONS] [VERSION]
```

Run `tfg` in a directory containing `.tf` files and it figures out the rest:

```
tfg                         # use the required_version constraint from .tf files
tfg 1.5.7                   # pin to an exact version
tfg --dir ./infra           # point at a different directory
tfg --dir ./infra -v        # verbose output showing the search process
```

If a `flake.nix` already exists and its pinned nixpkgs commit already satisfies
the constraint, `tfg` exits early and leaves it alone.

## GitHub token

The tool hits the GitHub API to search nixpkgs. It works without
authentication, but unauthenticated requests are limited to 60 per hour. With a
token that limit goes up to 5,000/hr, which matters when the commit history walk
kicks in.

You only need a fine-grained personal access token with read access to public
repos -- no write permissions, no specific repository access:

1. Go to Settings > Developer settings > Fine-grained personal access tokens
2. Create a new token
3. Under "Repository access", select **Public Repositories (read-only)**
4. No additional permissions are needed (nixpkgs is a public repository)
5. Export it:

```
export GITHUB_TOKEN=github_pat_...
```

You can also pass it directly with `--github-token`.

## How it works

`tfg` parses the `required_version` field from your Terraform configuration
blocks using an HCL parser. It then searches nixpkgs in two tiers:

**Tier 1 -- branch HEADs.** It checks the Terraform version at the tip of
`nixpkgs-unstable` and the five most recent `nixos-YY.MM` release branches.
This is fast (a handful of API calls) and covers most cases where you want a
current or recent Terraform.

**Tier 2 -- commit history.** If no branch HEAD satisfies the constraint, it
walks the commit history of the Terraform package file in nixpkgs (up to 100
commits). This finds older versions at the cost of more API calls, which is
where having a token helps.

Once a matching commit is found, `tfg` writes a `flake.nix` that pins
`nixpkgs` to that exact commit and exposes a dev shell with Terraform via
flake-parts. If a `flake.nix` already exists, it updates the nixpkgs input URL
in place rather than overwriting the whole file.

## License

MIT
