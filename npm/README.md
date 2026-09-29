# Tlegado npm packages

The release workflow builds one platform package per target:

- `tlegado-linux-x64`
- `tlegado-darwin-x64`
- `tlegado-darwin-arm64`
- `tlegado-win32-x64`

Each package contains the native `tlegado` executable and exposes it as the
`tlegado` command. GitHub Actions creates the tarballs and attaches them to a
tagged GitHub Release. Publishing to npm remains a manual step: download the
desired tarball from the release, then run `npm publish <file>.tgz` locally.
