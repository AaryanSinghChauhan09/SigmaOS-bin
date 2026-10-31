# SigmaOS binaries

This package contains Rust command-line entry points for SigmaOS and package-management experiments. The source tree defines binaries in `src/bin/`, including `sigmaos`, `sigpkg`, and `sigma_pkg`, and exposes a `SystemBinaries` library module.

The commands are evolving prototypes. The presence of a command or help text does not establish that package installation, repository synchronization, snapshots, rollback, or foreign package-manager operations are production-ready. Inspect the selected subcommand and its configured backends before use.

## Build

The manifest references sibling [`SigmaOS-kernel`](../SigmaOS-kernel). From the SigmaOS checkout:

```sh
cargo check --manifest-path SigmaOS-bin/Cargo.toml --bins
```

## License

GPL-3.0-or-later. See [`Cargo.toml`](Cargo.toml).
