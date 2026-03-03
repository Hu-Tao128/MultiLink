# Network and Firewall Troubleshooting

If a remote Ollama server fails from **Settings -> Servers -> Test connection**, MultiLink now shows an inline diagnostic and a suggested fix. This page documents the full checklist.

## Common causes

- `connection refused`: host reachable, but Ollama is not listening on `11434` externally.
- `timeout` / `filtered`: firewall blocks traffic (UFW/iptables/router ACL).
- invalid host like `0.0.0.0`: wildcard bind is not a client destination.

## Correct server bind

`0.0.0.0` is valid for **server bind**, not for client URLs.

- Good bind on server: `OLLAMA_HOST=0.0.0.0:11434`
- Good client URL in MultiLink: `http://192.168.x.x:11434` or `http://100.x.x.x:11434`

## systemd override for Ollama

```bash
sudo mkdir -p /etc/systemd/system/ollama.service.d
sudo nano /etc/systemd/system/ollama.service.d/override.conf
```

Put:

```ini
[Service]
Environment="OLLAMA_HOST=0.0.0.0:11434"
```

Reload and restart:

```bash
sudo systemctl daemon-reload
sudo systemctl restart ollama
sudo ss -tlnp | grep 11434
```

You should see `0.0.0.0:11434` (or `:::11434`) listening.

## UFW examples (LAN + Tailscale)

```bash
sudo ufw allow from 192.168.0.0/24 to any port 11434 proto tcp
sudo ufw allow from 100.64.0.0/10 to any port 11434 proto tcp
sudo ufw reload
sudo ufw status verbose
```

## Verify from client

```bash
curl http://192.168.0.33:11434/api/tags
curl http://100.124.176.21:11434/api/tags
```

If these work, MultiLink should pass the server test and list remote models.
