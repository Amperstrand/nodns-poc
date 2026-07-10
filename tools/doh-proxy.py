#!/usr/bin/env python3
"""
nodns premium DoH proxy.

Injects the X-Subscription header into DoH queries so browser-native
DoH clients (which can't send custom headers) can use the premium tier.

Usage:
    python3 doh-proxy.py --token YOUR_SUBSCRIPTION_TOKEN
    python3 doh-proxy.py --token YOUR_TOKEN --port 5353

Then configure your browser DoH to:
    http://localhost:5353/dns-query

Requires Python 3.9+. No external dependencies.
"""

import argparse
import http.server
import urllib.request
import urllib.error
import ssl
import sys

PREMIUM_URL = "https://dns.nodns.shop/dns-query/premium"


class DoHProxy(http.server.BaseHTTPRequestHandler):
    token = ""

    def do_POST(self):
        if "/dns-query" not in self.path:
            self.send_error(404)
            return

        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length) if length else b""

        req = urllib.request.Request(
            PREMIUM_URL,
            data=body,
            method="POST",
        )
        req.add_header("Content-Type", "application/dns-message")
        req.add_header("X-Subscription", self.token)

        ctx = ssl.create_default_context()
        try:
            resp = urllib.request.urlopen(req, context=ctx, timeout=10)
            data = resp.read()
            self.send_response(200)
            self.send_header("Content-Type", "application/dns-message")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(data)
        except urllib.error.HTTPError as e:
            self.send_response(e.code)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(
                f'{{"error":"upstream returned {e.code}"}}'.encode()
            )
        except Exception as e:
            self.send_response(502)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(f'{{"error":"{e}"}}'.encode())

    def do_GET(self):
        if self.path == "/" or self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"nodns DoH proxy running. Configure browser DoH to: http://localhost:")
            self.wfile.write(str(self.server.server_port).encode())
            self.wfile.write(b"/dns-query\n")
        else:
            self.send_error(404)

    def log_message(self, fmt, *args):
        sys.stderr.write(f"  {self.client_address[0]} {fmt % args}\n")


def main():
    parser = argparse.ArgumentParser(description="nodns premium DoH proxy")
    parser.add_argument("--token", required=True, help="Subscription token from /api/resolver/subscribe")
    parser.add_argument("--port", type=int, default=5353, help="Local port (default: 5353)")
    args = parser.parse_args()

    DoHProxy.token = args.token

    server = http.server.HTTPServer(("127.0.0.1", args.port), DoHProxy)
    print(f"nodns DoH proxy on http://127.0.0.1:{args.port}")
    print(f"Configure browser DoH to: http://127.0.0.1:{args.port}/dns-query")
    print(f"Subscription token: {args.token[:12]}...")
    print()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopped.")
        server.shutdown()


if __name__ == "__main__":
    main()
