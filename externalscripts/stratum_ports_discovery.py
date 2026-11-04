#!/usr/bin/env python3
import json
import re
import sys


def main():
    if len(sys.argv) > 2:
        raise SystemExit("Expected at most one comma-separated port list argument")

    # An omitted or empty argument intentionally disables discovery.
    ports_arg = sys.argv[1] if len(sys.argv) == 2 else ""
    if not ports_arg.strip():
        print(json.dumps({"data": []}))
        return

    ports = []
    seen_ports = set()
    for raw_port in ports_arg.split(","):
        port = raw_port.strip()
        if not re.fullmatch(r"[0-9]{1,5}", port) or not (1 <= int(port) <= 65535):
            raise SystemExit(f"Invalid Stratum port {port!r} in {ports_arg!r}")

        port_number = int(port)
        if port_number not in seen_ports:
            seen_ports.add(port_number)
            ports.append(port)

    print(json.dumps({"data": [{"{#STRATUM.PORT}": p} for p in ports]}))


if __name__ == "__main__":
    main()
