# Stratum v1 + Stratum v2 Pool Monitoring for Zabbix

This repository provides monitoring for Bitcoin mining pools using both **Stratum v1** and **Stratum v2** on **Zabbix 7.4+**.

The protocol versions are intentionally kept as separate Zabbix templates:

- `Template_Stratum_v1_Pool.yaml` for Stratum v1
- `Template_Stratum_v2_Pool.yaml` for Stratum v2
- link both templates to the same host when a pool supports both protocols

The templates use separate macro namespaces, so SV1 and SV2 can be configured independently on the same Zabbix host.

## Supported monitoring

### Stratum v1

- Plain TCP and TLS
- Multiple ports on one host through LLD discovery
- Per-port TLS and SNI configuration
- Configurable `mining.subscribe` user-agent, for example `cpuminer`
- Availability, subscribe, notify, extranonce and latency monitoring
- Per-port triggers and graphs

### Stratum v2

- Multiple ports on one host through LLD discovery
- TCP connectivity
- Stratum V2 Noise handshake
- Optional pool authority-key verification
- `SetupConnection` / `SetupConnection.Success`
- `OpenStandardMiningChannel` / `OpenStandardMiningChannel.Success`
- Detection of `NewMiningJob`
- Detection of `SetNewPrevHash`
- Protocol-stage error reporting
- End-to-end probe latency
- Static x86_64 Linux musl probe with no Rust runtime dependency

------------------------------------------------------------------------

# Architecture

## Stratum v1

The existing SV1 monitoring consists of:

- `stratum_v1.py` -- main Stratum v1 external check
- `stratum_ports_discovery.py` -- shared port-discovery helper for LLD
- `Template_Stratum_v1_Pool.yaml` -- Zabbix 7.4 multi-port SV1 template

## Stratum v2

The SV2 monitoring adds:

- `sv2-probe/` -- Rust source for the Stratum v2 probe
- `bin/stratum_v2_probe-linux-x86_64-musl` -- prebuilt static Linux binary
- `Template_Stratum_v2_Pool.yaml` -- Zabbix 7.4 multi-port SV2 template
- `.github/workflows/build-sv2-probe.yml` -- reproducible x86_64 musl build workflow

The existing `stratum_ports_discovery.py` is reused by both templates.

For each SV2 check, a single probe connection performs:

1. TCP connect
2. Stratum V2 Noise handshake
3. `SetupConnection` and waits for `SetupConnection.Success`
4. `OpenStandardMiningChannel` and waits for success
5. Briefly listens for `NewMiningJob` and `SetNewPrevHash`

The probe prints one JSON document. The Zabbix template stores that as one master external-check item and extracts the individual metrics as dependent items, so a monitoring interval does not create a separate SV2 handshake for every metric.

The probe does not mine and does not submit shares.

------------------------------------------------------------------------

# Requirements

Runtime requirements:

- Zabbix Server or Proxy 7.4+
- Python 3.6+ for the Python SV1 and LLD helper scripts
- Network access from the Zabbix server/proxy to the pool
- External checks enabled
- For SV2, the prebuilt `stratum_v2_probe` binary

Rust and Cargo are **not** required on the Zabbix server when using the prebuilt static SV2 binary. They are needed only on a machine that builds the probe from source.

------------------------------------------------------------------------

# Installation

## 1. Determine the ExternalScripts directory

Check the Zabbix configuration:

```bash
grep ^ExternalScripts /etc/zabbix/zabbix_server.conf
```

If `ExternalScripts` is not explicitly set, a common default is:

```text
/usr/lib/zabbix/externalscripts
```

Another possible configuration is:

```text
ExternalScripts=/etc/zabbix/scripts
```

All examples below use `/usr/lib/zabbix/externalscripts`. Adjust the path if your Zabbix installation uses another directory.

## 2. Install Stratum v1 scripts

```bash
cp stratum_v1.py /usr/lib/zabbix/externalscripts/
cp stratum_ports_discovery.py /usr/lib/zabbix/externalscripts/
chmod +x /usr/lib/zabbix/externalscripts/stratum_v1.py
chmod +x /usr/lib/zabbix/externalscripts/stratum_ports_discovery.py
```

If you use `/etc/zabbix/scripts` instead:

```bash
mkdir -p /etc/zabbix/scripts
cp stratum_v1.py /etc/zabbix/scripts/
cp stratum_ports_discovery.py /etc/zabbix/scripts/
chmod +x /etc/zabbix/scripts/stratum_v1.py
chmod +x /etc/zabbix/scripts/stratum_ports_discovery.py
```

## 3. Install the Stratum v2 probe

Using the prebuilt binary shipped in `bin/`:

```bash
install -m 0755 bin/stratum_v2_probe-linux-x86_64-musl \
  /usr/lib/zabbix/externalscripts/stratum_v2_probe
```

The binary is statically linked, so no Rust runtime or Cargo installation is required on the Zabbix server.

## 4. Restart Zabbix if needed

```bash
systemctl restart zabbix-server
```

or restart `zabbix-proxy` when the external checks run on a proxy.

------------------------------------------------------------------------

# Test the checks manually

Always test using the same user that Zabbix uses.

## Test port discovery

```bash
sudo -u zabbix /usr/lib/zabbix/externalscripts/stratum_ports_discovery.py "3333,443"
```

Expected output:

```json
{"data":[{"{#STRATUM.PORT}":"3333"},{"{#STRATUM.PORT}":"443"}]}
```

## Test Stratum v1

```bash
sudo -u zabbix /usr/lib/zabbix/externalscripts/stratum_v1.py \
  pool.example.com 3333 alive --timeout 2
```

Expected result:

```text
1
```

## Test Stratum v2

```bash
sudo -u zabbix /usr/lib/zabbix/externalscripts/stratum_v2_probe \
  pool.example.com 23330 \
  --timeout 3 \
  --user 'POOL-SPECIFIC-USER-IDENTITY'
```

Example successful result:

```json
{"alive":1,"tcp_ok":1,"noise_ok":1,"setup_ok":1,"channel_ok":1,"job_seen":1,"prevhash_seen":1,"latency_ms":250,"used_version":2,"setup_flags":1,"channel_id":1,"authority_verified":0,"error_stage":"","error_code":""}
```

The meaning of `user_identity` is pool-specific. For pools that use a Bitcoin payout address plus worker name, it can look like:

```text
bc1q....zabbix
```

Do not assume that a generic value such as `zabbix` is accepted by every pool.

------------------------------------------------------------------------

# Template Import

Import the templates you need:

```text
Template_Stratum_v1_Pool.yaml
Template_Stratum_v2_Pool.yaml
```

In Zabbix 7.4 this is available through the template import UI.

Use only the SV1 template for an SV1-only pool, only the SV2 template for an SV2-only pool, or link both templates when the same pool supports both protocols.

------------------------------------------------------------------------

# Host Configuration

1. Create a host representing the pool IP address or DNS name.
2. Set the host interface to the pool address; the templates use `{HOST.CONN}`.
3. Link `Template Stratum v1 Pool`, `Template Stratum v2 Pool`, or both.
4. Configure the protocol-specific macros described below.

------------------------------------------------------------------------

# Stratum v1 Configuration

## Multi-Port Configuration (LLD)

Set:

```text
{$STRATUM.PORTS}
```

Example:

```text
3333,4333,443
```

The template automatically creates SV1 items, triggers and graphs for each discovered port.

## Per-Port TLS Configuration

TLS can be enabled only for selected ports by using Zabbix macro contexts.

Example:

```text
{$STRATUM.EXTRA_ARGS:"3333"} =
{$STRATUM.EXTRA_ARGS:"443"} = --tls --sni pool.example.com
```

The user-agent can also be overridden by port:

```text
{$STRATUM.USERAGENT:"443"} = cpuminer
```

## Stratum v1 Macros

| Macro | Description |
| --- | --- |
| `{$STRATUM.PORTS}` | Comma-separated SV1 ports |
| `{$STRATUM.TIMEOUT}` | Script timeout, default 2 seconds |
| `{$STRATUM.USERAGENT}` | `mining.subscribe` client string |
| `{$STRATUM.EXTRA_ARGS}` | Additional CLI flags, including TLS options |
| `{$STRATUM.LATENCY.WARN_MS}` | Elevated-latency threshold |
| `{$STRATUM.LATENCY.HIGH_MS}` | High-latency threshold |
| `{$STRATUM.NODATA}` | No-data period |

Keep `{$STRATUM.TIMEOUT}` lower than the Zabbix server/proxy `Timeout=` value.

Recommended example:

```text
Timeout=10
{$STRATUM.TIMEOUT}=2
```

## Stratum v1 Items Created Per Port

- Stratum alive
- Stratum subscribe OK
- Stratum latency in milliseconds
- Stratum notify seen
- Stratum extranonce2 size

## Stratum v1 Triggers Per Port

- Stratum is down
- No Stratum data
- Stratum subscribe failed
- Stratum latency high
- Stratum latency elevated

## Stratum v1 Graph Per Port

Each port gets a **Stratum status + latency** graph containing:

- Alive (0/1)
- Subscribe OK (0/1)
- Latency in milliseconds on the right axis

------------------------------------------------------------------------

# Stratum v2 Configuration

## Multi-Port Configuration (LLD)

Set:

```text
{$STRATUMV2.PORTS}
```

Example:

```text
23330
```

Multiple ports may be specified as a comma-separated list.

## User identity

Set a `user_identity` value accepted by the monitored pool:

```text
{$STRATUMV2.USER_IDENTITY}
```

The required format is pool-specific. For example, a pool may expect a Bitcoin payout address with an optional worker suffix:

```text
bc1q....zabbix
```

Context macros may be used if different ports require different identities:

```text
{$STRATUMV2.USER_IDENTITY:"23330"}=bc1q....zabbix
```

## Pool authority key

The pool authority key is optional, but configuring it enables authenticated Noise verification instead of encryption without pool-identity verification.

Set:

```text
{$STRATUMV2.AUTHORITY_KEY}
```

The value is the Stratum V2 authority public key published by the pool in **Base58Check** form. It is typically about 51 characters long.

Example per-port context macro:

```text
{$STRATUMV2.AUTHORITY_KEY:"23330"}=9bT...
```

When an authority key is configured and the Noise handshake succeeds, the probe reports:

```text
"authority_verified":1
```

An invalid, expired, not-yet-valid, or incorrectly signed Noise certificate causes the authenticated Noise handshake to fail.

### Certificate validity timestamps

The current probe validates the signed Noise certificate through the SRI Noise implementation but does not yet export the certificate's exact `valid_from` and `not_valid_after` timestamps as Zabbix metrics.

Therefore the current version detects a certificate failure when the handshake fails, but it cannot yet warn before certificate expiry. Exporting the validity timestamps can be added in a later version.

## Stratum v2 Macros

| Macro | Description |
| --- | --- |
| `{$STRATUMV2.PORTS}` | Comma-separated SV2 ports |
| `{$STRATUMV2.TIMEOUT}` | Probe timeout, default 3 seconds |
| `{$STRATUMV2.USER_IDENTITY}` | Pool-specific SV2 `user_identity` |
| `{$STRATUMV2.HASHRATE}` | Nominal probe hashrate in H/s; default 1 TH/s |
| `{$STRATUMV2.AUTHORITY_KEY}` | Optional pool authority key in SV2 Base58Check format |
| `{$STRATUMV2.LATENCY.WARN_MS}` | Elevated-latency threshold |
| `{$STRATUMV2.LATENCY.HIGH_MS}` | High-latency threshold |
| `{$STRATUMV2.NODATA}` | No-data period |

Context macros can be used for `USER_IDENTITY`, `HASHRATE`, and `AUTHORITY_KEY` on individual ports.

## Stratum v2 Items Created Per Port

The template runs one master probe item and creates dependent metrics for:

- Overall SV2 alive state
- TCP connectivity
- Noise handshake result
- `SetupConnection` result
- Standard mining-channel result
- End-to-end latency
- `NewMiningJob` observed during the short probe
- `SetNewPrevHash` observed during the short probe
- Negotiated SV2 version
- Authority verification state
- Error stage
- Error code

`job_seen` and `prevhash_seen` are informational. A short monitoring connection is not guaranteed to see a fresh message on every run, so availability triggers should not depend on them.

## Stratum v2 Triggers Per Port

The SV2 template includes triggers for:

- Stratum V2 down
- No Stratum V2 data
- TCP reachable but Noise handshake failing
- `SetupConnection` failing
- Mining channel cannot be opened
- Elevated latency
- High latency

------------------------------------------------------------------------

# Stratum v2 Probe Result Semantics

A successful result may look like:

```json
{
  "alive": 1,
  "tcp_ok": 1,
  "noise_ok": 1,
  "setup_ok": 1,
  "channel_ok": 1,
  "job_seen": 1,
  "prevhash_seen": 1,
  "latency_ms": 250,
  "used_version": 2,
  "setup_flags": 1,
  "channel_id": 1,
  "authority_verified": 1,
  "error_stage": "",
  "error_code": ""
}
```

Semantics:

- `alive=1` -- Noise, `SetupConnection`, and Standard Channel open all succeeded
- `tcp_ok=1` -- TCP connection succeeded
- `noise_ok=1` -- Noise handshake succeeded
- `setup_ok=1` -- server returned `SetupConnection.Success`
- `channel_ok=1` -- server returned `OpenStandardMiningChannel.Success`
- `job_seen=1` -- `NewMiningJob` was observed during the probe
- `prevhash_seen=1` -- `SetNewPrevHash` was observed during the probe
- `authority_verified=1` -- an authority key was supplied and authenticated Noise verification succeeded
- `error_stage` -- failing stage such as `tcp`, `noise`, `setup`, or `channel`
- `error_code` -- protocol or local error useful for troubleshooting

------------------------------------------------------------------------

# Building the Stratum v2 Probe

Rust is required only on the build machine.

## Ubuntu / Debian build host

Install the musl tools and add the Rust target:

```bash
sudo apt-get update
sudo apt-get install -y musl-tools file
rustup target add x86_64-unknown-linux-musl
```

Build:

```bash
cd sv2-probe
cargo build --release --target x86_64-unknown-linux-musl
```

The resulting executable is:

```text
sv2-probe/target/x86_64-unknown-linux-musl/release/stratum_v2_probe
```

Verify that it is static:

```bash
file target/x86_64-unknown-linux-musl/release/stratum_v2_probe
ldd target/x86_64-unknown-linux-musl/release/stratum_v2_probe || true
```

A correct build should report a statically linked executable, or `ldd` should report that it is not a dynamic executable.

Stage the release binary in the repository:

```bash
cp target/x86_64-unknown-linux-musl/release/stratum_v2_probe \
  ../bin/stratum_v2_probe-linux-x86_64-musl
chmod 0755 ../bin/stratum_v2_probe-linux-x86_64-musl
sha256sum ../bin/stratum_v2_probe-linux-x86_64-musl \
  > ../bin/stratum_v2_probe-linux-x86_64-musl.sha256
```

If the project ships prebuilt probes, commit both the source and the resulting binary/checksum.

## GitHub Actions build

`.github/workflows/build-sv2-probe.yml` builds the `x86_64-unknown-linux-musl` executable and verifies that the resulting binary is static.

The workflow uploads the binary and checksum as a GitHub Actions artifact. A normal PR build deliberately does **not** commit the generated binary back into the branch automatically.

After a successful run:

1. Open the workflow run in GitHub Actions.
2. Open its **Summary** page.
3. Download the artifact from the **Artifacts** section.
4. Put the binary and `.sha256` file under `bin/` if they are intended to be committed to the repository.

## Cargo.lock

Commit `sv2-probe/Cargo.lock` so future CI builds use the same resolved dependency versions.

To create it initially:

```bash
cd sv2-probe
cargo generate-lockfile
```

Once committed, CI should build with `cargo build --locked`.

------------------------------------------------------------------------

# Troubleshooting

## Timeout while executing an external check

Increase the Zabbix server/proxy timeout if necessary, while keeping the protocol-specific probe timeout lower.

For example:

```text
Timeout=10
{$STRATUM.TIMEOUT}=2
{$STRATUMV2.TIMEOUT}=3
```

## Works in CLI but not in Zabbix

Check:

- the executable/script is in the directory configured by `ExternalScripts=`
- file permissions allow the Zabbix user to execute it
- SELinux/AppArmor policy if applicable
- host `{HOST.CONN}` resolves to the expected pool address
- the same command succeeds when run as the `zabbix` user
- the required TLS flags are configured for SV1
- `{$STRATUMV2.USER_IDENTITY}` is accepted by the SV2 pool
- the SV2 authority key is copied exactly in Base58Check form if authority verification is enabled

## SV2 reports `unknown-user`

The pool rejected the `user_identity` supplied in `OpenStandardMiningChannel`.

Configure `{$STRATUMV2.USER_IDENTITY}` using the format required by that pool. On pools that identify miners by payout address, this may need to be a valid Bitcoin address rather than a generic monitoring username.

## SV2 TCP succeeds but Noise fails

If an authority key is configured, confirm that it is the exact Stratum V2 Base58Check authority key published by the pool. Do not convert it to hex.

An authenticated Noise failure can also indicate a certificate that is invalid, expired, not yet valid, or not signed by the configured authority key.

------------------------------------------------------------------------

# Important Notes

- This project does **not** mine Bitcoin.
- The checks establish short-lived protocol connections only.
- No shares are submitted.
- SV1 and SV2 are intentionally separate templates and can be linked independently.
- The monitoring is designed for pool availability and protocol-health checks.
- SV2 `job_seen` and `prevhash_seen` are informational rather than availability criteria.
