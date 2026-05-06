#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fcntl
import ipaddress
import json
import os
import selectors
import socket
import struct
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Iterable


CTLIOCGINFO = 0xC0644E03
UTUN_OPT_IFNAME = 2
UTUN_CONTROL_NAME = b"com.apple.net.utun_control"
DEFAULT_RADIO_MTU = 1445
DEFAULT_TUN_MTU = DEFAULT_RADIO_MTU - 2


@dataclass
class Counters:
    tun_packets_in: int = 0
    tun_bytes_in: int = 0
    tunnel_datagrams_out: int = 0
    tunnel_bytes_out: int = 0
    tunnel_datagrams_in: int = 0
    tunnel_bytes_in: int = 0
    tun_packets_out: int = 0
    tun_bytes_out: int = 0
    keepalives_out: int = 0
    corrupt_messages: int = 0
    truncated_messages: int = 0
    dropped_packets: int = 0


class Aggregator:
    def __init__(self, max_size: int, timeout_s: float):
        self.max_size = max_size
        self.timeout_s = timeout_s
        self.parts: list[bytes] = []
        self.size = 0
        self.deadline: float | None = None

    def add(self, packet: bytes, now: float) -> list[bytes]:
        record = len(packet).to_bytes(2, "big") + packet
        if len(record) > self.max_size:
            raise ValueError(f"packet record exceeds aggregate MTU: {len(record)} > {self.max_size}")

        out = []
        if self.size and self.size + len(record) > self.max_size:
            out.append(self.flush())

        self.parts.append(record)
        self.size += len(record)
        if self.timeout_s <= 0:
            out.append(self.flush())
        elif self.deadline is None:
            self.deadline = now + self.timeout_s
        return [msg for msg in out if msg]

    def flush_due(self, now: float) -> bytes | None:
        if self.deadline is not None and now >= self.deadline:
            return self.flush()
        return None

    def flush(self) -> bytes:
        data = b"".join(self.parts)
        self.parts.clear()
        self.size = 0
        self.deadline = None
        return data


def parse_endpoint(value: str) -> tuple[str, int]:
    host, sep, port_s = value.rpartition(":")
    if not sep or not host:
        raise argparse.ArgumentTypeError(f"expected HOST:PORT, got {value!r}")
    return host, int(port_s, 10)


def parse_tunnel_message(message: bytes, counters: Counters) -> Iterable[bytes]:
    if not message:
        return

    offset = 0
    while offset < len(message):
        if len(message) - offset < 2:
            counters.corrupt_messages += 1
            return
        packet_len = int.from_bytes(message[offset : offset + 2], "big")
        offset += 2
        if len(message) - offset < packet_len:
            counters.truncated_messages += 1
            return
        yield message[offset : offset + packet_len]
        offset += packet_len


def utun_af_header(packet: bytes) -> bytes:
    if not packet:
        raise ValueError("empty IP packet")
    version = packet[0] >> 4
    if version == 4:
        family = socket.AF_INET
    elif version == 6:
        family = socket.AF_INET6
    else:
        raise ValueError(f"unsupported IP version {version}")
    return struct.pack("!I", family)


def strip_utun_header(frame: bytes) -> bytes:
    if len(frame) < 5:
        raise ValueError("utun frame too short")
    family = int.from_bytes(frame[:4], "big")
    if family not in (socket.AF_INET, socket.AF_INET6):
        raise ValueError(f"unsupported utun address family {family}")
    return frame[4:]


def open_utun(unit: int) -> tuple[socket.socket, str]:
    sock = socket.socket(socket.PF_SYSTEM, socket.SOCK_DGRAM, socket.SYSPROTO_CONTROL)
    ctl_info = struct.pack("I96s", 0, UTUN_CONTROL_NAME)
    ctl_info = fcntl.ioctl(sock.fileno(), CTLIOCGINFO, ctl_info)
    ctl_id = struct.unpack("I96s", ctl_info)[0]
    sock.connect((ctl_id, unit))
    ifname = (
        sock.getsockopt(socket.SYSPROTO_CONTROL, UTUN_OPT_IFNAME, 64)
        .split(b"\0", 1)[0]
        .decode("ascii")
    )
    sock.setblocking(False)
    return sock, ifname


def netmask(prefix_len: int) -> str:
    return str(ipaddress.IPv4Network(f"0.0.0.0/{prefix_len}").netmask)


def configure_interface(ifname: str, local_ip: str, peer_ip: str, prefix_len: int, mtu: int) -> None:
    subprocess.run(
        [
            "/sbin/ifconfig",
            ifname,
            "inet",
            local_ip,
            peer_ip,
            "netmask",
            netmask(prefix_len),
            "mtu",
            str(mtu),
            "up",
        ],
        check=True,
    )
    subprocess.run(["/sbin/route", "-n", "add", "-host", peer_ip, "-interface", ifname], check=False)


def log(event: str, **fields: object) -> None:
    record = {"ts": time.time(), "event": event, **fields}
    print(json.dumps(record, sort_keys=True), file=sys.stderr, flush=True)


def run(args: argparse.Namespace) -> int:
    if sys.platform != "darwin":
        raise SystemExit("wfb-mac-wf-tun.py currently supports macOS utun only")

    counters = Counters()
    tun, ifname = open_utun(args.utun_unit)
    if args.configure:
        configure_interface(ifname, args.local_ip, args.peer_ip, args.prefix_len, args.tun_mtu)

    tx_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    rx_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    rx_sock.bind(args.rx_bind)
    rx_sock.setblocking(False)

    selector = selectors.DefaultSelector()
    selector.register(tun, selectors.EVENT_READ, "tun")
    selector.register(rx_sock, selectors.EVENT_READ, "udp")

    aggregator = Aggregator(args.radio_mtu, args.agg_timeout_ms / 1000.0)
    next_keepalive = time.monotonic() + args.keepalive_interval_s
    next_stats = time.monotonic() + args.stats_interval_s if args.stats_interval_s > 0 else None

    log(
        "started",
        ifname=ifname,
        local_ip=args.local_ip,
        peer_ip=args.peer_ip,
        tun_mtu=args.tun_mtu,
        radio_mtu=args.radio_mtu,
        tx_peer=f"{args.tx_peer[0]}:{args.tx_peer[1]}",
        rx_bind=f"{args.rx_bind[0]}:{args.rx_bind[1]}",
    )

    while True:
        now = time.monotonic()
        deadlines = [next_keepalive]
        if aggregator.deadline is not None:
            deadlines.append(aggregator.deadline)
        if next_stats is not None:
            deadlines.append(next_stats)
        timeout = max(0.0, min(deadlines) - now)

        for key, _ in selector.select(timeout):
            if key.data == "tun":
                try:
                    packet = strip_utun_header(tun.recv(args.tun_mtu + 4))
                    counters.tun_packets_in += 1
                    counters.tun_bytes_in += len(packet)
                    for message in aggregator.add(packet, time.monotonic()):
                        tx_sock.sendto(message, args.tx_peer)
                        counters.tunnel_datagrams_out += 1
                        counters.tunnel_bytes_out += len(message)
                except BlockingIOError:
                    pass
                except Exception as exc:
                    counters.dropped_packets += 1
                    log("tun_packet_drop", error=str(exc))
            elif key.data == "udp":
                try:
                    message, _addr = rx_sock.recvfrom(args.radio_mtu + 256)
                    counters.tunnel_datagrams_in += 1
                    counters.tunnel_bytes_in += len(message)
                    for packet in parse_tunnel_message(message, counters):
                        tun.send(utun_af_header(packet) + packet)
                        counters.tun_packets_out += 1
                        counters.tun_bytes_out += len(packet)
                except BlockingIOError:
                    pass
                except Exception as exc:
                    counters.dropped_packets += 1
                    log("udp_packet_drop", error=str(exc))

        now = time.monotonic()
        due = aggregator.flush_due(now)
        if due:
            tx_sock.sendto(due, args.tx_peer)
            counters.tunnel_datagrams_out += 1
            counters.tunnel_bytes_out += len(due)

        if now >= next_keepalive:
            tx_sock.sendto(b"", args.tx_peer)
            counters.keepalives_out += 1
            next_keepalive = now + args.keepalive_interval_s

        if next_stats is not None and now >= next_stats:
            log("stats", **counters.__dict__)
            next_stats = now + args.stats_interval_s


def self_test() -> int:
    counters = Counters()
    msg = b"\x00\x03abc\x00\x02de"
    assert list(parse_tunnel_message(msg, counters)) == [b"abc", b"de"]
    assert counters.corrupt_messages == 0
    assert counters.truncated_messages == 0

    assert list(parse_tunnel_message(b"\x00\x04abc", counters)) == []
    assert counters.truncated_messages == 1

    packet = bytes([0x45, 0, 0, 20]) + b"\0" * 16
    assert strip_utun_header(utun_af_header(packet) + packet) == packet

    agg = Aggregator(12, 0)
    assert agg.add(b"abc", 0) == [b"\x00\x03abc"]
    agg = Aggregator(12, 0.005)
    assert agg.add(b"abc", 0) == []
    assert agg.add(b"de", 0) == []
    assert agg.flush() == b"\x00\x03abc\x00\x02de"

    print("self-test ok")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Bridge macOS utun IP packets to WFB-NG tunnel UDP messages."
    )
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--utun-unit", type=int, default=0, help="0 asks macOS to allocate the next utun")
    parser.add_argument("--local-ip", default="10.5.0.1")
    parser.add_argument("--peer-ip", default="10.5.0.2")
    parser.add_argument("--prefix-len", type=int, default=24)
    parser.add_argument("--tun-mtu", type=int, default=DEFAULT_TUN_MTU)
    parser.add_argument("--radio-mtu", type=int, default=DEFAULT_RADIO_MTU)
    parser.add_argument("--tx-peer", type=parse_endpoint, default=("127.0.0.1", 56020))
    parser.add_argument("--rx-bind", type=parse_endpoint, default=("127.0.0.1", 56021))
    parser.add_argument("--agg-timeout-ms", type=float, default=5.0)
    parser.add_argument("--keepalive-interval-s", type=float, default=0.5)
    parser.add_argument("--stats-interval-s", type=float, default=5.0)
    parser.add_argument("--no-configure", dest="configure", action="store_false")
    parser.set_defaults(configure=True)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
