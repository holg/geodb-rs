#!/usr/bin/env python3
"""Talks to the board over Ethernet (UDP port 7880): finds it by broadcast, then

    board_ctl.py            the network test: info, 200 pings (round trip, loss), a 1000 x 512 byte blast
    board_ctl.py info       uptime, frame rate, build
    board_ctl.py ping [N]   N pings (default 200)
    board_ctl.py blast [N]  N datagrams of 512 bytes from the board (default 1000)
    board_ctl.py reset      restarts the board (no ST-LINK needed)

Give the address yourself with --host 192.168.x.y when the broadcast does not get through.
"""
import socket
import sys
import time

PORT = 7880


def open_socket():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    s.settimeout(0.5)
    return s


def ask(s, dest, text, tries=3):
    for _ in range(tries):
        s.sendto(text.encode(), dest)
        try:
            data, peer = s.recvfrom(2048)
            return data.decode("ascii", "replace"), peer
        except socket.timeout:
            pass
    return None, None


def find(s, host):
    if host:
        reply, peer = ask(s, (host, PORT), "!info")
    else:
        reply, peer = ask(s, ("255.255.255.255", PORT), "!info")
    if not reply:
        sys.exit("no answer from the board (cable? DHCP? same network? try --host IP)")
    return peer[0], reply


def ping(s, ip, n):
    rtts, lost = [], 0
    for i in range(n):
        t = time.perf_counter()
        s.sendto(f"!ping {i}".encode(), (ip, PORT))
        try:
            while True:
                data, _ = s.recvfrom(2048)
                if data.decode("ascii", "replace").strip() == f"!pong {i}":
                    rtts.append((time.perf_counter() - t) * 1000)
                    break
        except socket.timeout:
            lost += 1
        time.sleep(0.005)
    if rtts:
        rtts.sort()
        print(f"ping: {n} sent, {lost} lost, rtt ms min {rtts[0]:.2f} median {rtts[len(rtts)//2]:.2f} "
              f"p95 {rtts[int(len(rtts)*0.95)-1]:.2f} max {rtts[-1]:.2f}")
    else:
        print(f"ping: all {n} lost")
    return lost


def blast(s, ip, n):
    s.settimeout(1.0)
    s.sendto(f"!blast {n}".encode(), (ip, PORT))
    seen, size, t0 = set(), 0, None
    done = False
    while not done:
        try:
            data, _ = s.recvfrom(2048)
        except socket.timeout:
            break
        if t0 is None:
            t0 = time.perf_counter()
        if data.startswith(b"!blast done"):
            done = True
        elif data.startswith(b"!blast "):
            seen.add(int(data.split()[1]))
            size += len(data)
    dt = max(time.perf_counter() - (t0 or time.perf_counter()), 1e-6)
    print(f"blast: {len(seen)} of {n} datagrams received ({n - len(seen)} lost), "
          f"{size / 1024:.0f} KB in {dt:.2f} s = {size * 8 / dt / 1e6:.2f} Mbit/s")
    return n - len(seen)


def main():
    args = sys.argv[1:]
    host = None
    if "--host" in args:
        i = args.index("--host")
        host = args[i + 1]
        del args[i:i + 2]
    cmd = args[0] if args else "test"
    count = int(args[1]) if len(args) > 1 else None
    s = open_socket()
    ip, info = find(s, host)
    print(f"board at {ip}: {info}")
    if cmd in ("test", "ping"):
        ping(s, ip, count or 200)
    if cmd in ("test", "blast"):
        blast(s, ip, count or 1000)
    if cmd == "test":
        print("after:", ask(s, (ip, PORT), "!info")[0])
    if cmd == "reset":
        print(ask(s, (ip, PORT), "!reset", tries=1)[0])


if __name__ == "__main__":
    main()
