# Tlegado npm packages

The release workflow builds one universal `tlegado` package containing the
Linux x64, macOS Intel, macOS arm64, and Windows x64 binaries. Its `tlegado`
command selects the native binary for the current machine. GitHub Actions
attaches this tarball and the individual binaries to a tagged GitHub Release.
Publishing to npm remains a manual step: download the tarball and run
`npm publish tlegado-<version>.tgz` locally once.
