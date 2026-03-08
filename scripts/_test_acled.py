#!/usr/bin/env python3
"""Temporary script to debug ACLED API endpoints."""
import httpx
import json
import base64

# Get token
resp = httpx.post(
    "https://acleddata.com/oauth/token",
    headers={"Content-Type": "application/x-www-form-urlencoded"},
    data={
        "username": "leon.der.1234@gmail.com",
        "password": "TileWorldMap1!",
        "grant_type": "password",
        "client_id": "acled",
    },
    timeout=30,
)
body = resp.json()
print("Token response keys:", list(body.keys()))
token = body["access_token"]

# Decode JWT payload
parts = token.split(".")
payload = base64.urlsafe_b64decode(parts[1] + "==")
print("JWT payload:", json.loads(payload))

# The POST confirms /api/acled/read exists (405 Method Not Allowed, GET only)
# but GET returns 403 "Access denied" from Drupal auth layer.
# Try with various auth approaches and required params:
tests = [
    # Maybe email is still needed as a param alongside Bearer token
    {"url": "https://acleddata.com/api/acled/read", "params": {"limit": "2", "email": "leon.der.1234@gmail.com"}},
    # Try with terms=accept
    {"url": "https://acleddata.com/api/acled/read", "params": {"limit": "2", "terms": "accept"}},
    # Try with key param set to the token  
    {"url": "https://acleddata.com/api/acled/read", "params": {"limit": "2", "key": token}},
    # Try with key=token and email
    {"url": "https://acleddata.com/api/acled/read", "params": {"limit": "2", "key": token, "email": "leon.der.1234@gmail.com"}},
    # Cookie-based auth 
    {"url": "https://acleddata.com/api/acled/read", "params": {"limit": "2"}, "cookies": {"access_token": token}},
    # Maybe X-Api-Key style
    {"url": "https://acleddata.com/api/acled/read", "params": {"limit": "2"}, "extra_headers": {"X-Api-Key": token}},
]
for t in tests:
    try:
        hdrs = {"Authorization": f"Bearer {token}", "Accept": "application/json"}
        if "extra_headers" in t:
            hdrs.update(t["extra_headers"])
        r = httpx.get(
            t["url"],
            headers=hdrs,
            params=t.get("params", {}),
            cookies=t.get("cookies"),
            timeout=15,
            follow_redirects=True,
        )
        print(f"\nGET {t['url']} params={t.get('params',{})} => {r.status_code}: {r.text[:300]}")
    except Exception as e:
        print(f"\nGET {t['url']} => ERROR: {e}")
