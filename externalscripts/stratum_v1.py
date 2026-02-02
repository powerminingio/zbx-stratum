#!/usr/bin/env python3
import argparse
import json
import shlex
import socket
import ssl
import sys
import time


def send_req(sock, req_obj):
    msg = (json.dumps(req_obj) + "\n").encode("utf-8")
    sock.sendall(msg)


def recv_line(sock, deadline_ts, pending=b""):
    buf = pending
    while True:
        if time.time() > deadline_ts:
            raise TimeoutError("timeout waiting for response")
        if b"\n" in buf:
            line, rest = buf.split(b"\n", 1)
            return line.decode("utf-8", errors="replace"), rest
        chunk = sock.recv(4096)
        if not chunk:
            raise ConnectionError("connection closed by peer")
        buf += chunk


def connect(host, port, timeout, use_tls, sni=None, insecure=False):
    raw = socket.create_connection((host, port), timeout=timeout)
    raw.settimeout(timeout)

    if not use_tls:
        return raw

    ctx = ssl.create_default_context()
    if insecure:
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE

    server_hostname = sni or host
    tls_sock = ctx.wrap_socket(raw, server_hostname=server_hostname)
    tls_sock.settimeout(timeout)
    return tls_sock


def parse_args_with_extra(argv):
    """
    Support Zabbix macro-friendly --extra "<flags>" which comes as a single token.
    We split it with shlex and re-parse with a validating parser.
    """
    base = argparse.ArgumentParser(add_help=False)
    base.add_argument("host")
    base.add_argument("port", type=int)
    base.add_argument("metric")
    base.add_argument("--timeout", type=float, default=3.0)
    base.add_argument("--tls", action="store_true")
    base.add_argument("--sni", default=None)
    base.add_argument("--insecure", action="store_true")
    base.add_argument("--user", default=None)
    base.add_argument("--passw", default="x")
    base.add_argument("--useragent", default="zabbix-stratum-check")
    base.add_argument("--extra", default="", help="Extra args as a single string (macro-friendly)")

    args1, _unknown1 = base.parse_known_args(argv)

    extra_tokens = shlex.split(args1.extra) if args1.extra else []

    cleaned = []
    skip_next = False
    for tok in argv:
        if skip_next:
            skip_next = False
            continue
        if tok == "--extra":
            skip_next = True
            continue
        cleaned.append(tok)

    merged = cleaned + extra_tokens

    p = argparse.ArgumentParser(description="Stratum v1 external check for Zabbix")
    p.add_argument("host")
    p.add_argument("port", type=int)
    p.add_argument(
        "metric",
        choices=[
            "alive",
            "latency_ms",
            "subscribe_ok",
            "authorize_ok",
            "notify_seen",
            "extranonce1_len",
            "extranonce2_size",
            "session_id",
        ],
    )
    p.add_argument("--timeout", type=float, default=3.0)
    p.add_argument("--tls", action="store_true", help="Use TLS (stratum+tls)")
    p.add_argument("--sni", default=None, help="Override TLS SNI hostname")
    p.add_argument("--insecure", action="store_true", help="Disable TLS cert validation")
    p.add_argument("--user", default=None, help="Optional worker username for mining.authorize")
    p.add_argument("--passw", default="x", help="Optional worker password for mining.authorize (default: x)")
    p.add_argument("--useragent", default="zabbix-stratum-check",
                   help='User-Agent string used in mining.subscribe (e.g. "cpuminer")')

    return p.parse_args(merged)


def main():
    try:
        args = parse_args_with_extra(sys.argv[1:])
    except Exception:
        print("0")
        sys.exit(0)

    start = time.time()
    deadline = start + args.timeout

    req_id = 1
    subscribe_ok = 0
    authorize_ok = 0
    notify_seen = 0
    extranonce1_len = 0
    extranonce2_size = 0
    session_id = 0

    try:
        sock = connect(args.host, args.port, args.timeout, args.tls, sni=args.sni, insecure=args.insecure)

        # 1) mining.subscribe (User-Agent / client id is args.useragent)
        send_req(sock, {"id": req_id, "method": "mining.subscribe", "params": [args.useragent]})
        req_id += 1

        line, pending = recv_line(sock, deadline, pending=b"")
        resp = json.loads(line)

        if resp.get("error") is None and resp.get("result"):
            subscribe_ok = 1
            try:
                result = resp["result"]
                extranonce1 = result[1]
                extranonce1_len = len(extranonce1) if isinstance(extranonce1, str) else 0
                extranonce2_size = int(result[2]) if len(result) > 2 else 0
            except Exception:
                pass
            session_id = resp.get("id") or 0

        # 2) Optional authorize
        if args.user:
            send_req(sock, {"id": req_id, "method": "mining.authorize", "params": [args.user, args.passw]})
            req_id += 1
            line2, pending = recv_line(sock, deadline, pending=pending)
            resp2 = json.loads(line2)
            if resp2.get("error") is None and resp2.get("result") is True:
                authorize_ok = 1

        # 3) Only wait for mining.notify when requested
        if args.metric == "notify_seen":
            look_deadline = min(deadline, time.time() + 0.8)
            leftover = pending

            while time.time() < look_deadline:
                if b"\n" in leftover:
                    l3, leftover = leftover.split(b"\n", 1)
                    try:
                        msg = json.loads(l3.decode("utf-8", errors="replace"))
                        if msg.get("method") == "mining.notify":
                            notify_seen = 1
                            break
                    except Exception:
                        pass
                    continue

                try:
                    chunk = sock.recv(4096)
                    if not chunk:
                        break
                    leftover += chunk
                except socket.timeout:
                    break

        latency_ms = int(round((time.time() - start) * 1000.0))

        if args.metric == "alive":
            print("1" if subscribe_ok == 1 else "0")
        elif args.metric == "latency_ms":
            print(str(latency_ms))
        elif args.metric == "subscribe_ok":
            print(str(subscribe_ok))
        elif args.metric == "authorize_ok":
            print(str(authorize_ok))
        elif args.metric == "notify_seen":
            print(str(notify_seen))
        elif args.metric == "extranonce1_len":
            print(str(extranonce1_len))
        elif args.metric == "extranonce2_size":
            print(str(extranonce2_size))
        elif args.metric == "session_id":
            print(str(session_id))
        else:
            print("0")

        try:
            sock.close()
        except Exception:
            pass

    except Exception:
        print("0")
        sys.exit(0)


if __name__ == "__main__":
    main()

