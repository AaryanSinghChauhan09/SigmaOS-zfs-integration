# ZFS Dataset Layout (FreeBSD/Omarchy-inspired)

- `sigmaos/ROOT` — OS datasets (encryption optional)
- `sigmaos/ROOT/var` — variable data
- `sigmaos/HOME` — user data, can be shared across installs
- `sigmaos/VARLOG` — logs
- Snapshots: `zfs-auto-snapshot` hourly/daily/weekly, sent to backup target
- Boot environments via `zfs snapshot` + bootloader entries
