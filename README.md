### Building

Dependencies are:
```
fontconfig
libxkbcommon
wayland
vulkan-loader
```
build with `cargo build --release`

### Support

Right now only niri is supported, more may or may not come in the future.

### For NixOS/Home manager
Add the flake as an input
```nix
paperwing = {
  url = "github:Spaceey1/PaperWing";
  inputs.nixpkgs.follows = "nixpkgs"; #optional
};
```
Then add it as a NixOS package in your packages list.
```nix
environment.systemPackages = [
  inputs.paperwing.packages.x86_64-linux.paperwing
];
```
Or in home manager
```nix
home.packages = [
  inputs.paperwing.packages.x86_64-linux.paperwing
];
```
If you want stylix integration set `programs.paperwing.enable = true` in home manager.

Remember to pass in inputs or the PaperWing flake as params to your modules.

If you don't feel like changing your configuration you can also try the game out with just
```
nix run github:Spaceey1/PaperWing
```
