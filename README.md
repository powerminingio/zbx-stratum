# Stratum v1 Pool Monitoring for Zabbix

This repository provides a **Stratum v1 monitoring solution for Zabbix
7.4+** to monitor Bitcoin mining pools using the native Stratum
protocol.

It supports:

-   Plain TCP and TLS
-   Multiple ports on the same host (LLD discovery)
-   Per-port TLS / SNI configuration
-   Configurable user-agent (e.g. `cpuminer`)
-   Production-safe triggers and graphs

------------------------------------------------------------------------

# Architecture

This solution consists of:

-   `stratum_v1.py` -- Main Stratum external check
-   `stratum_ports_discovery.py` -- Port discovery helper (LLD)
-   `Template_Stratum_v1_Pool.yaml` -- Zabbix 7.4 template (multi-port)

------------------------------------------------------------------------

# Requirements

-   Zabbix Server or Proxy 7.4+
-   Python 3.6+
-   Network access from server/proxy to pool
-   External checks enabled

------------------------------------------------------------------------

# Installation

## 1️⃣ Install Scripts

Determine your ExternalScripts directory:

``` bash
grep ^ExternalScripts /etc/zabbix/zabbix_server.conf
```

If not set, default is usually:

    /usr/lib/zabbix/externalscripts

You may alternatively set:

    ExternalScripts=/etc/zabbix/scripts

### Copy both scripts:

``` bash
mkdir -p /etc/zabbix/scripts

cp stratum_v1.py /etc/zabbix/scripts/
cp stratum_ports_discovery.py /etc/zabbix/scripts/

chmod +x /etc/zabbix/scripts/stratum_v1.py
chmod +x /etc/zabbix/scripts/stratum_ports_discovery.py
```

If using default directory instead:

``` bash
cp stratum_v1.py /usr/lib/zabbix/externalscripts/
cp stratum_ports_discovery.py /usr/lib/zabbix/externalscripts/
chmod +x /usr/lib/zabbix/externalscripts/*.py
```

Restart Zabbix:

``` bash
systemctl restart zabbix-server
```

(or `zabbix-proxy`)

------------------------------------------------------------------------

## 2️⃣ Test Scripts Manually

Always test as the `zabbix` user.

### Test discovery script

``` bash
sudo -u zabbix stratum_ports_discovery.py "3333,443"
```

Expected output:

``` json
{"data":[{"{#STRATUM.PORT}":"3333"},{"{#STRATUM.PORT}":"443"}]}
```

### Test Stratum check

``` bash
sudo -u zabbix stratum_v1.py pool.example.com 3333 alive --timeout 2
```

Expected output:

    1

------------------------------------------------------------------------

# Template Import

Import:

    Template_Stratum_v1_Pool.yaml

Zabbix UI:

    Configuration → Templates → Import

------------------------------------------------------------------------

# Host Configuration

1.  Create a host representing the pool IP/DNS.
2.  Set Host Interface to pool address (used by `{HOST.CONN}`).
3.  Link template: **Template Stratum v1 Pool**

------------------------------------------------------------------------

# Multi-Port Configuration (LLD)

Instead of one port, you now configure:

    {$STRATUM.PORTS}

Example:

    3333,4333,443

The template automatically creates items, triggers and graphs per port.

------------------------------------------------------------------------

# Per-Port TLS Configuration

You can configure TLS only for specific ports using macro context.

Example:

    {$STRATUM.EXTRA_ARGS:"3333"} =
    {$STRATUM.EXTRA_ARGS:"443"} = --tls --sni pool.example.com

You can also override user-agent per port:

    {$STRATUM.USERAGENT:"443"} = cpuminer

------------------------------------------------------------------------

# Main Macros

  Macro                          Description
  ------------------------------ --------------------------------
  `{$STRATUM.PORTS}`             Comma-separated ports
  `{$STRATUM.TIMEOUT}`           Script timeout (default 2)
  `{$STRATUM.USERAGENT}`         mining.subscribe client string
  `{$STRATUM.EXTRA_ARGS}`        Extra CLI flags
  `{$STRATUM.LATENCY.WARN_MS}`   Latency warning threshold
  `{$STRATUM.LATENCY.HIGH_MS}`   Latency high threshold
  `{$STRATUM.NODATA}`            No-data period

⚠️ Keep `{$STRATUM.TIMEOUT}` lower than Zabbix `Timeout=` in config.

Recommended:

    Timeout=10
    {$STRATUM.TIMEOUT}=2

------------------------------------------------------------------------

# Items Created Per Port

For each discovered port:

-   Stratum alive
-   Stratum subscribe ok
-   Stratum latency (ms)
-   Stratum notify seen
-   Stratum extranonce2 size

------------------------------------------------------------------------

# Triggers Per Port

-   Stratum is down (\>30s)
-   No Stratum data
-   Stratum subscribe failed
-   Stratum latency high
-   Stratum latency elevated

------------------------------------------------------------------------

# Graph Per Port

Each port gets:

**Stratum status + latency**

-   Alive (0/1)
-   Subscribe ok (0/1)
-   Latency (ms, right axis)

------------------------------------------------------------------------

# Troubleshooting

## Timeout while executing shell script

Increase:

    Timeout=10

And ensure:

    {$STRATUM.TIMEOUT}=2

------------------------------------------------------------------------

## Works in CLI but not in Zabbix

Check:

-   Script location matches `ExternalScripts=`
-   Script executable
-   SELinux/AppArmor
-   Testing with correct TLS flags
-   Running test as `zabbix` user

------------------------------------------------------------------------

# Important Notes

-   This does NOT mine.
-   It performs a short handshake only.
-   No shares are submitted.
-   Designed for availability & protocol monitoring.
