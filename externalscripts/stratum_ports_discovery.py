#!/usr/bin/env python3
import sys
import json

def main():
    # Expect a single argument: comma-separated ports
    if len(sys.argv) < 2:
        print(json.dumps({"data": []}))
        return

    ports_arg = sys.argv[1]

    if not ports_arg:
        print(json.dumps({"data": []}))
        return

    ports = [p.strip() for p in ports_arg.split(",") if p.strip()]

    data = []
    for port in ports:
        # Basic validation: numeric port
        if port.isdigit():
            data.append({"{#STRATUM.PORT}": port})

    print(json.dumps({"data": data}))

if __name__ == "__main__":
    main()
