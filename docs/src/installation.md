# Installation

Vex supports Linux, macOS and Windows.

<!-- ## Install via Snap (Linux + macOS only) -->
<!---->
<!-- The easiest way to install `vex` is via the [snap][vex-snap]. -->
<!-- On Linux and macOS, open a terminal and run the following— -->
<!-- ```bash -->
<!-- sudo snap install vex -->
<!---->
<!-- # If vex will be run on removable media, run this— -->
<!-- sudo snap connect vex:removable-storage -->
<!-- ``` -->
<!---->
<!-- Test the installation by running `vex`. -->

<!-- ## Install via cargo -->
<!---->
<!-- Use this option if snaps are unavailable on your system. -->
<!---->
Assuming that [cargo][cargo] is installed, open a terminal and run the following—
```bash
cargo install --git https://github.com/TheSignPainter98/vex
```

Test the installation by running `vex`.
If `vex` appears unavailable, ensure that `~/.cargo/bin/` is present in your `$PATH` and then retry.

<!-- ## Install from source -->

<!-- Use this option if you would like to contribute to `vex`. -->

<!-- Assuming that [cargo][cargo] is installed, open a terminal and run the following— -->
<!-- ```bash -->
<!-- git clone https://github.com/TheSignPainter98/vex -->
<!-- cd vex -->
<!-- cargo build -->
<!-- ``` -->
<!---->
<!-- This will create a binary in `target/debug/`. -->

[cargo]: https://doc.rust-lang.org/cargo/getting-started/installation.html
<!-- [vex-snap]: https://snapcraft.io/vex -->
