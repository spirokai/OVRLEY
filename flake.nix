{
  description = "OVRLEY activity video overlays";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/674a2028661e550a97400c1cc992b9cca9a668a1";
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-darwin,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          unstablePkgs = pkgsFor system;
          hostPkgs =
            if system == "x86_64-darwin" then
              import nixpkgs-darwin {
                inherit system;
                overlays = [ rust-overlay.overlays.default ];
              }
            else
              unstablePkgs;
          emscripten =
            if system == "x86_64-darwin" then
              hostPkgs.callPackage ./nix/emscripten.nix { }
            else
              unstablePkgs.emscripten;
          pnpm_10 =
            if system == "x86_64-darwin" then
              hostPkgs.pnpm_10.overrideAttrs (old: rec {
                version = "10.34.7";
                src = hostPkgs.fetchurl {
                  url = "https://registry.npmjs.org/pnpm/-/pnpm-${version}.tgz";
                  hash = "sha256-xyn2U44AXSNIiZEEZv0cTESv+wso6obZP8+IqhrvppQ=";
                };
                meta = old.meta // {
                  knownVulnerabilities = [ ];
                };
              })
            else
              hostPkgs.pnpm_10;
          ovrley = hostPkgs.callPackage ./nix/ovrley.nix {
            inherit emscripten pnpm_10;
            src = self;
          };
        in
        {
          default = ovrley;
          inherit ovrley;
        }
      );

      apps = forAllSystems (
        system:
        let
          ovrley = self.packages.${system}.ovrley;
        in
        {
          default = {
            type = "app";
            program = "${ovrley}/bin/OVRLEY";
            meta = ovrley.meta;
          };
        }
      );
    };
}
