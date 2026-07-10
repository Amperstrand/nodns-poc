# tools/

Utility scripts for the nodns project.

## doh-proxy.py

Local DoH proxy for the nodns premium resolver. Injects the `X-Subscription`
header into DoH queries so browser-native DoH clients (which can't send custom
headers) can use the premium tier.

**Usage:**

```bash
# Get a subscription token first:
curl -X POST https://dns.nodns.shop/api/resolver/subscribe \
  -H "X-Cashu: cashuB..."

# Then run the proxy:
python3 tools/doh-proxy.py --token YOUR_SUBSCRIPTION_TOKEN

# Configure browser DoH to:
#   http://localhost:5353/dns-query
```

Requires Python 3.9+. No external dependencies.
