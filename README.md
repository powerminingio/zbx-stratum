# Stratum v1 Pool Monitoring for Zabbix

This repository provides a **Stratum v1 monitoring solution for Zabbix
7.4** to check the health and performance of a Bitcoin mining pool using
the Stratum protocol itself.

It uses a **Python external check** that speaks Stratum v1
(`mining.subscribe`, optional `mining.authorize`) and a **Zabbix
template** with items, triggers, and graphs.

------------------------------------------------------------------------

## Features

-   Native **Stratum v1 protocol check** (no pool-specific APIs)
-   Works with **plain TCP or TLS**
-   Configurable **user agent** (e.g. `cpuminer`)
-   Measures:
    -   Pool availability (`mining.subscribe`)
    -   Subscribe success
    -   Latency
    -   Notify visibility (for debugging)
    -   Extranonce size
-   Production-safe triggers (low noise)
-   Combined graph: **status + latency**
-   Fully compatible with **Zabbix 7.4 YAML import**

------------------------------------------------------------------------

## Requirements

-   Zabbix **Server or Proxy** 7.4+
-   Python **3.6+**
-   Network access from Zabbix server/proxy to the pool endpoint
-   Script executed as **External check**

------------------------------------------------------------------------

## Files

-   `stratum_v1.py` -- Python external check script
-   `Template_Stratum_v1_Pool.yaml` -- Zabbix 7.4 template (items,
    triggers, graph)

------------------------------------------------------------------------

## Installation

### 1) Install the script

Place the script on the **Zabbix server or proxy**:

``` bash
mkdir -p /etc/zabbix/scripts
cp stratum_v1.py /etc/zabbix/scripts/stratum_v1.py
chmod +x /etc/zabbix/scripts/stratum_v1.py
```

Either:

**Option A (recommended):** point Zabbix to this directory\
Edit `/etc/zabbix/zabbix_server.conf` (or `zabbix_proxy.conf`):

``` conf
ExternalScripts=/etc/zabbix/scripts
Timeout=10
```

Restart:

``` bash
systemctl restart zabbix-server
# or zabbix-proxy
```

**Option B:** symlink into existing ExternalScripts directory:

``` bash
ln -s /etc/zabbix/scripts/stratum_v1.py /usr/lib/zabbix/externalscripts/stratum_v1.py
```

------------------------------------------------------------------------

### 2) Test manually (important)

Run as the `zabbix` user from the server/proxy:

``` bash
sudo -u zabbix /etc/zabbix/scripts/stratum_v1.py pool.example.com 3333 alive --timeout 2
```

Expected output:

    1

Latency test:

``` bash
sudo -u zabbix /etc/zabbix/scripts/stratum_v1.py pool.example.com 3333 latency_ms --timeout 2
```

------------------------------------------------------------------------

### 3) Import the template

In Zabbix UI:

    Configuration → Templates → Import

Import `Template_Stratum_v1_Pool.yaml`.

------------------------------------------------------------------------

### 4) Link template to a host

-   Create or select a host representing the **pool endpoint**
-   Set **Host interface** to the pool IP or DNS name
-   Link template: **Template Stratum v1 Pool**

------------------------------------------------------------------------

## Configuration (Macros)

All behavior is controlled via template/host macros.

### Required

  Macro                  Description                Example
  ---------------------- -------------------------- ---------
  `{$STRATUM.PORT}`      Stratum port               `3333`
  `{$STRATUM.TIMEOUT}`   Script timeout (seconds)   `2`

> ⚠️ Keep this **lower** than Zabbix `Timeout` (e.g. 2 vs 10).

------------------------------------------------------------------------

### Optional (TLS, auth, etc.)

  -------------------------------------------------------------------------------------------
  Macro                     Description                      Example
  ------------------------- -------------------------------- --------------------------------
  `{$STRATUM.EXTRA_ARGS}`   Extra CLI flags                  `--tls --sni pool.example.com`

  `{$STRATUM.USERAGENT}`    Stratum user agent               `cpuminer`
  -------------------------------------------------------------------------------------------

Examples:

**TLS pool**

    {$STRATUM.EXTRA_ARGS} = --tls

**TLS with SNI**

    {$STRATUM.EXTRA_ARGS} = --tls --sni pool.example.com

**Authorize (if pool requires it)**

    {$STRATUM.EXTRA_ARGS} = --user workername --passw x

------------------------------------------------------------------------

### User Agent (cpuminer spoofing)

The script sends this value in `mining.subscribe`:

    {$STRATUM.USERAGENT} = cpuminer

You may also use:

    cpuminer/2.5.1

Many pools log or expose this value in stats.

------------------------------------------------------------------------

## Items Collected

  Item                       Description
  -------------------------- --------------------------------------
  Stratum alive              1 if `mining.subscribe` succeeds
  Stratum subscribe ok       Explicit subscribe success
  Stratum latency (ms)       End-to-end handshake latency
  Stratum notify seen        Whether `mining.notify` was observed
  Stratum extranonce2 size   Protocol sanity check

------------------------------------------------------------------------

## Triggers

### Availability

-   **Stratum is down (\>30s)** -- HIGH\
-   **No Stratum data** -- AVERAGE

### Protocol / performance

-   **Stratum subscribe failed** -- HIGH
-   **Stratum latency is high** -- WARNING
-   **Stratum latency is elevated** -- INFO

------------------------------------------------------------------------

## Graphs

### "Stratum status + latency"

Single graph combining: - **Stratum alive** (0/1) - **Stratum subscribe
ok** (0/1) - **Stratum latency** (ms, right axis)

------------------------------------------------------------------------

## Troubleshooting

### External check timeout in logs

    Timeout while executing a shell script

Fix: - Set `Timeout=10` in `zabbix_server.conf` - Set
`{$STRATUM.TIMEOUT}=2`

------------------------------------------------------------------------

### Works in CLI but not in Zabbix

Common causes: - Script not in `ExternalScripts` - Wrong permissions -
Zabbix timeout too low - Pool requires TLS or SNI

Always test as:

``` bash
sudo -u zabbix stratum_v1.py ...
```

------------------------------------------------------------------------

## Notes / Limitations

-   This is **not mining** --- no shares are submitted.
-   No persistent connection; each check is a short handshake.
-   Some pools only send `mining.notify` after authorize or job timing.
-   Designed for **pool health**, not miner accounting.
