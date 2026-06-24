# codeHarness SPEC

## §G goal
Rust CLI. Fast spin-up disposable encrypted Linux desktop VM in window. No data leak. Instant nuke.

## §C constraints
- Rust CLI. macOS Apple Silicon primary, Linux bonus.
- Backend QEMU only. mac=HVF, linux=KVM. Child proc + QMP. No hypervisor bindings.
- Security top priority.
- All artifacts confined to user VM dir.
- Secrets never on argv.
- Guest: Debian + latest KDE + VS Code, package list = config.

## §I surfaces
- CLI cmds: `create` `start` `stop` `nuke` `list` `config` `image build`
- config file: `~/.config/codeharness/<vm>.toml`
- redirect rules file: `redirect.toml` (hot-reload)
- QMP unix socket (internal control)
- image inputs: `images/packages.base.toml` + config `[packages].extra`

## §V invariants
V1 | disk = raw LUKS (aes-256-xts, sha256, high iter-time) over a sparse payload file; LUKS header DETACHED in a separate small file. (Empirical: qcow2 embeds its LUKS header so can't detach; raw-luks chosen for crypto-erase. Sparse via filesystem. qcow2-in-luks/backing-from-golden deferred to T7 if copy cost hurts.)
V2 | nuke = shred detached header then rm qcow2; after nuke disk unopenable even w/ correct key.
V3 | key source per-VM ∈ {passphrase, keyfile}; secrets never in argv/logs; zeroized in mem.
V4 | net mode ∈ {none, open, allow, redirect}; none = `restrict=on`, zero host+internet reach from guest. First boot (cloud-init provisioning, before `.provisioned` marker) forces full `open` net regardless of mode; configured mode applies on every boot after.
V5 | redirect = no-MITM L4; route by DNS+SNI, byte-splice to target (e.g. binance.com→host:8080); no TLS decrypt, no CA in guest.
V6 | redirect gateway fail-closed: unknown/unparseable/ECH traffic dropped, never leak to internet.
V7 | redirect rules hot-reload w/o VM restart; in-flight conns keep old rules, new conns use new.
V8 | isolation default-off: no 9p/virtfs, no clipboard unless config opt-in.
V9 | all artifacts (qcow2, header, sockets, logs) live under VM dir; nothing written outside.
V10 | GUI window: virtio-gpu + `cocoa`(mac)/`gtk`(linux). GL accel (`gl=es`/`on` + virtio-gpu-gl) only when qemu built with OpenGL AND config [display].gl=true; default gl=off + virtio-gpu-pci (software framebuffer) since stock brew qemu lacks OpenGL.
V11 | config validated at load; invalid rejected w/ clear error before any QEMU spawn.
V12 | runtime deps (qemu-img, qemu-system-<arch>; passt optional) detected before any VM op; never spawn with required deps missing; `doctor --install` installs via host pkg mgr (brew/apt/dnf/pacman) only on explicit user consent.

## §T tasks
id|status|desc|cites
T1|x|scaffold workspace + cli + config schema/validate|V11
T2|x|encrypted disk create, detached LUKS header, key sources (boot→T4)|V1,V3
T3|x|nuke crypto-erase (shred header + rm)|V2
T4|x|qemu process mgmt + QMP client + GUI display per-OS (GL window validated at T7)|V10
T5|x|net none/open (slirp; passt deferred to linux/T8)|V4
T6|x|net allow host:port allowlist — FOLDED into T8 (host proxy ruleset: permit listed host:port, drop rest)|V4
T7|x|debian cloud image -> encrypted disk + cloud-init seed (KDE+vscode+pkg list); GUI first-boot = interactive|V11,I.images
T8|x|egress proxy (slirp restrict+guestfwd -> host HTTP/CONNECT proxy; route by hostname; allow/redirect/open/none; hot-reload rules; redirect=L4 splice no-MITM). NOTE: slirp blocks transparent gateway; proxy-respecting egress only, rest fail-closed|V5,V6,V7
T9|.|isolation hardening + secret/log/artifact audit|V8,V9,V3
T10|.|tests: crypto round-trip, nuke unrecoverable, 4 net modes, gateway fail-closed|V1,V2,V4,V5,V6
T11|x|dependency preflight + doctor install (qemu/passt, brew/apt/dnf/pacman)|V12

## §B bugs
id|date|cause|fix
