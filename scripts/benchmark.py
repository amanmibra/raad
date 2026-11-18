#!/usr/bin/env python3
"""Reproducible synthetic monorepo measurement; requires Python 3, Cargo and Git."""
import json
import os
from pathlib import Path
import random
import socket
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]

def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.DEVNULL)

def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]

def main():
    subprocess.run(["cargo", "build", "--quiet", "--example", "benchmark"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="raad-bench-") as directory:
        base = Path(directory)
        repo = base / "monorepo"
        repo.mkdir()
        git(repo, "init", "-b", "main")
        git(repo, "config", "user.email", "benchmark@example.invalid")
        git(repo, "config", "user.name", "raad benchmark")
        git(repo, "config", "uploadpack.allowFilter", "true")
        git(repo, "config", "uploadpack.allowAnySHA1InWant", "true")
        (repo / "sops").mkdir()
        (repo / "sops/policy.md").write_text("# Policy\n\nReview refunds within 30 days.\n")
        (repo / "sops/config.json").write_text('{"enabled":true}\n')
        (repo / "unrelated").mkdir()
        generator = random.Random(42)
        size = 64 * 1024 * 1024
        for i in range(64):
            (repo / f"unrelated/{i}.bin").write_bytes(generator.randbytes(1024 * 1024))
        git(repo, "add", ".")
        git(repo, "commit", "-qm", "synthetic monorepo")
        daemon_port = free_port()
        daemon = subprocess.Popen(["git", "daemon", "--reuseaddr", "--export-all", f"--base-path={base}", "--listen=127.0.0.1", f"--port={daemon_port}", str(base)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        proxy = socket.socket()
        proxy.bind(("127.0.0.1", 0))
        proxy.listen()
        proxy.settimeout(0.2)
        proxy_port = proxy.getsockname()[1]
        stop = threading.Event()
        counts = [0, 0]
        mutex = threading.Lock()
        def copy(source, dest, index):
            try:
                while data := source.recv(65536):
                    with mutex:
                        counts[index] += len(data)
                    dest.sendall(data)
            except OSError:
                pass
            finally:
                try:
                    dest.shutdown(socket.SHUT_WR)
                except OSError:
                    pass
        def handle(client):
            with client, socket.create_connection(("127.0.0.1", daemon_port)) as backend:
                a = threading.Thread(target=copy, args=(client, backend, 0))
                b = threading.Thread(target=copy, args=(backend, client, 1))
                a.start(); b.start(); a.join(); b.join()
        def accept():
            while not stop.is_set():
                try:
                    client, _ = proxy.accept()
                except socket.timeout:
                    continue
                threading.Thread(target=handle, args=(client,), daemon=True).start()
        worker = threading.Thread(target=accept)
        worker.start()
        try:
            for _ in range(50):
                try:
                    with socket.create_connection(("127.0.0.1", daemon_port), timeout=0.1):
                        break
                except OSError:
                    time.sleep(0.05)
            env = dict(os.environ, RAAD_BENCH_ORIGIN=f"git://127.0.0.1:{proxy_port}/monorepo")
            result = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/benchmark")], env=env, cwd=ROOT))
            result.update(unrelated_worktree_bytes=size, network_client_bytes=counts[0], network_server_bytes=counts[1])
            assert result["network_server_bytes"] < size // 4, result
            assert result["cache_bytes"] < size // 4, result
            print(json.dumps(result, indent=2))
        finally:
            stop.set(); worker.join(); proxy.close(); daemon.terminate(); daemon.wait()

if __name__ == "__main__":
    main()
