# Publishing to winget

These manifests let users install with `winget install cuong21951.cron`. To get
the package into the public winget catalog, it must be submitted (once per
version) to [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs).

## Easiest path: `wingetcreate`

[`wingetcreate`](https://github.com/microsoft/winget-create) generates the
manifests, fills in the SHA256 automatically, and opens the PR for you.

```powershell
winget install Microsoft.WingetCreate

# After a GitHub release exists for the tag:
wingetcreate new https://github.com/cuong21951/cron/releases/download/v0.1.0/cron-0.1.0-x86_64-pc-windows-msvc.exe
# Answer the prompts (identifier: cuong21951.cron, type: portable, command: cron),
# then let it submit the PR to microsoft/winget-pkgs.
```

To update an existing package for a new release:

```powershell
wingetcreate update cuong21951.cron --version 0.2.0 --urls <new-exe-url> --submit
```

## Manual path

1. Cut a GitHub release (push a `v*` tag — the Release workflow builds the exe
   and prints its SHA256).
2. Edit the three YAML files in this folder: set `PackageVersion`, the
   `InstallerUrl`, and `InstallerSha256`.
3. Validate locally:
   ```powershell
   winget validate --manifest .\winget
   winget install --manifest .\winget   # optional local install test
   ```
4. Fork `microsoft/winget-pkgs`, copy the files to
   `manifests/c/cuong21951/cron/<version>/`, and open a PR. Automated checks run
   on the PR; once they pass and a maintainer approves, the package goes live.

> Note: the publisher account must verify ownership for first-time submissions.
> Follow the prompts on the winget-pkgs PR if asked.
