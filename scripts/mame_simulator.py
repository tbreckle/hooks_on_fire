#!/usr/bin/env python3
"""Simple TCP server that simulates MAME output for testing hof-blaze."""

import random
import socket
import threading
import time

HOST = "0.0.0.0"
PORT = 8000

MESSAGES = [
    "LampStart=1",
    "LampStart=0",
    "LampCoin=1",
    "LampCoin=0",
    "Recoil=1",
    "Recoil=0",
    "Vibrate=1",
    "Vibrate=0",
]


def handle_client(conn, addr):
    print(f"[+] Connection from {addr}")
    try:
        conn.sendall(b"mame_start=test\n")
        print(f"    Sent: mame_start=test")
        while True:
            delay = random.uniform(5, 10)
            time.sleep(delay)
            msg = random.choice(MESSAGES)
            conn.sendall(f"{msg}\n".encode())
            print(f"    Sent: {msg}")
    except (BrokenPipeError, ConnectionResetError, OSError):
        print(f"[-] Connection closed: {addr}")
    finally:
        conn.close()


def main():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as srv:
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind((HOST, PORT))
        srv.listen()
        print(f"MAME simulator listening on {HOST}:{PORT}")
        while True:
            conn, addr = srv.accept()
            threading.Thread(target=handle_client, args=(conn, addr), daemon=True).start()


if __name__ == "__main__":
    main()
